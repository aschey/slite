use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::{fs, io};

use clap::{ArgAction, Args, CommandFactory, Parser, ValueEnum};
use clap_complete::{generate, Shell};
use color_eyre::Report;
use confique::{toml, Config};
use minus::Pager;
use normpath::PathExt;
use notify_debouncer_mini::DebouncedEvent;
use owo_colors::OwoColorize;
use regex::Regex;
use rusqlite::Connection;
use serde::de::Visitor;
use serde::{Deserialize, Serialize};
use slite::error::InitializationError;
use slite::tui::run_tui;
use slite::{read_extension_dir, read_sql_files, Migrator, Options, SqlPrinter};
use tokio::sync::mpsc;
use tracing::metadata::LevelFilter;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::prelude::*;
use tracing_subscriber::reload::{self, Handle};
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{Layer, Registry};
use tracing_tree2::HierarchicalLayer;

#[derive(ValueEnum, Clone)]
enum SchemaType {
    Source,
    Target,
}

#[derive(ValueEnum, Clone)]
enum Migrate {
    Run,
    DryRun,
    Script,
}

#[derive(ValueEnum, Clone)]
enum AppConfig {
    Generate,
}

#[derive(clap::Subcommand, Clone)]
#[command(author, version, about)]
enum AppCommand {
    Migrate { migrate: Migrate },
    Config { config: AppConfig },
    Diff,
    Print { from: SchemaType },
    Completions { shell: Shell },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SerdeRegex(#[serde(with = "serde_regex")] pub(crate) Regex);

impl PartialEq for SerdeRegex {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_str() == other.0.as_str()
    }
}

impl Eq for SerdeRegex {}

#[derive(thiserror::Error, Debug)]
#[error("Error parsing log level: {0} is not a valid value")]
pub struct LevelParseError(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerdeLevel(LevelFilter);

impl FromStr for SerdeLevel {
    type Err = LevelParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(SerdeLevel(match s.to_lowercase().as_str() {
            "trace" => LevelFilter::TRACE,
            "debug" => LevelFilter::DEBUG,
            "info" => LevelFilter::INFO,
            "warn" => LevelFilter::WARN,
            "error" => LevelFilter::ERROR,
            _ => return Err(LevelParseError(s.to_owned())),
        }))
    }
}

impl Serialize for SerdeLevel {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self.0 {
            LevelFilter::TRACE => serializer.serialize_str("trace"),
            LevelFilter::DEBUG => serializer.serialize_str("debug"),
            LevelFilter::INFO => serializer.serialize_str("info"),
            LevelFilter::WARN => serializer.serialize_str("warn"),
            LevelFilter::ERROR => serializer.serialize_str("error"),
            _ => serializer.serialize_str(""),
        }
    }
}

impl<'de> Deserialize<'de> for SerdeLevel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct LevelDeserializer;

        impl<'de> Visitor<'de> for LevelDeserializer {
            type Value = SerdeLevel;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("A valid log level")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                SerdeLevel::from_str(v).map_err(|e| E::custom(e.to_string()))
            }
        }

        deserializer.deserialize_str(LevelDeserializer)
    }
}

#[derive(clap::Parser)]
struct Cli {
    #[command(subcommand)]
    command: Option<AppCommand>,
    #[command(flatten)]
    config: Conf,
}

fn source_parser(val: &str) -> Result<PathBuf, Report> {
    let path = PathBuf::from(val.to_owned());
    match path.try_exists() {
        Ok(true) => Ok(path),
        Ok(false) => Err(color_eyre::eyre::eyre!("Path does not exist")),
        Err(e) => Err(color_eyre::eyre::eyre!("{e}")),
    }
}

fn destination_parser(val: &str) -> Result<PathBuf, Report> {
    let path = PathBuf::from(val.to_owned());
    match (path.try_exists(), path.is_file()) {
        (Ok(true), false) => Err(color_eyre::eyre::eyre!("Destination must be a file")),
        (Ok(_), _) => Ok(path),
        (Err(e), _) => Err(color_eyre::eyre::eyre!("{e}")),
    }
}

fn regex_parser(val: &str) -> Result<SerdeRegex, regex::Error> {
    Ok(SerdeRegex(Regex::new(val)?))
}

#[derive(Debug, Clone, Default, Args, confique::Config, Serialize, Deserialize)]
pub struct Conf {
    #[config(env = "SLITE_SOURCE_DIR")]
    #[arg(short, long, value_parser = source_parser)]
    pub source: Option<PathBuf>,
    #[config(env = "SLITE_PRE_MIGRATION_DIR")]
    #[arg(short='e', long, value_parser = source_parser)]
    pub pre_migration: Option<PathBuf>,
    #[config(env = "SLITE_POST_MIGRATION_DIR")]
    #[arg(short='o', long, value_parser = source_parser)]
    pub post_migration: Option<PathBuf>,
    #[config(env = "SLITE_TARGET_DB")]
    #[arg(short, long, value_parser = destination_parser)]
    pub target: Option<PathBuf>,
    #[config(env = "SLITE_EXTENSION_DIR")]
    #[arg(short='d', long, value_parser = source_parser)]
    pub extension_dir: Option<PathBuf>,
    #[config(env = "SLITE_IGNORE_PATTERN")]
    #[arg(short, long, value_parser = regex_parser)]
    pub ignore: Option<SerdeRegex>,
    #[config(env = "SLITE_LOG_LEVEL")]
    #[arg(short, long)]
    pub log_level: Option<SerdeLevel>,
    #[config(env = "SLITE_USE_PAGER")]
    #[arg(short, long, action = ArgAction::SetTrue)]
    pub pager: Option<bool>,
}

impl Conf {
    fn migrator_config_changed(&self, other: &Self) -> bool {
        self.extension_dir != other.extension_dir
            || self.ignore != other.ignore
            || self.pre_migration != other.pre_migration
            || self.post_migration != other.post_migration
    }
}

pub struct App {
    cli: Cli,
    source: PathBuf,
    target: PathBuf,
    schema: Vec<String>,
    config: slite::Config,
    log_level: LevelFilter,
    pager: Option<Pager>,
    cli_config: Conf,
}

impl App {
    pub fn from_args() -> Result<Self, Report> {
        owo_colors::set_override(atty::is(atty::Stream::Stdout));
        color_eyre::install()?;

        let cli = Cli::parse();
        let cli_config = cli.config.clone();
        let cli_config_ = cli_config.clone();
        let partial = confique_partial_conf::PartialConf {
            source: cli_config.source,
            target: cli_config.target,
            extension_dir: cli_config.extension_dir,
            ignore: cli_config.ignore,
            log_level: cli_config.log_level,
            pager: cli_config.pager,
            pre_migration: cli_config.pre_migration,
            post_migration: cli_config.post_migration,
        };

        let direct_path = PathBuf::from("./slite.toml");
        let path = if direct_path.exists() {
            Some(direct_path)
        } else {
            let git_root = match gix_discover::upwards_opts(
                ".",
                gix_discover::upwards::Options {
                    ceiling_dirs: vec![PathBuf::from("../../../..")],
                    ..Default::default()
                },
            ) {
                Ok((gix_discover::repository::Path::LinkedWorkTree { git_dir, .. }, _)) => {
                    Some(git_dir)
                }
                Ok((gix_discover::repository::Path::Repository(git_dir), _)) => Some(git_dir),
                Ok((gix_discover::repository::Path::WorkTree(git_dir), _)) => Some(git_dir),
                Err(_) => None,
            };
            match git_root {
                Some(git_root) => {
                    let path = git_root.join("slite.toml");
                    if path.exists() { Some(path) } else { None }
                }
                None => None,
            }
        };
        let mut conf_builder = Conf::builder().preloaded(partial).env();
        if let Some(path) = path {
            conf_builder = conf_builder.file(path);
        }
        let conf = conf_builder.load().unwrap();

        let source = conf.source.unwrap_or_default();
        let target = conf.target.unwrap_or_default();

        let extensions = conf
            .extension_dir
            .map(read_extension_dir)
            .unwrap()
            .unwrap_or_default();

        let ignore = conf.ignore.map(|i| i.0);
        let before_migration = conf.pre_migration.map(read_sql_files).unwrap_or_default();
        let after_migration = conf.post_migration.map(read_sql_files).unwrap_or_default();
        let config = slite::Config {
            extensions,
            ignore,
            before_migration,
            after_migration,
        };
        let log_level = conf.log_level.unwrap_or(SerdeLevel(LevelFilter::INFO));
        let schema = read_sql_files(&source);

        let pager = if conf.pager.unwrap_or_default()
            && cli.command.is_some()
            && atty::is(atty::Stream::Stdout)
        {
            let output = minus::Pager::new();

            let output_ = output.clone();
            tokio::task::spawn_blocking(move || minus::dynamic_paging(output_));
            Some(output)
        } else {
            None
        };

        Ok(Self {
            cli,
            source,
            target,
            schema,
            config,
            pager,
            cli_config: cli_config_,
            log_level: log_level.0,
        })
    }

    pub async fn run(self) -> Result<(), Report> {
        run_tui().await?;
        Ok(())
    }
}
