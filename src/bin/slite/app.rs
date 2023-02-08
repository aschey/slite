use crate::app_tui::run_tui;
use clap::{Args, Parser, ValueEnum};
use color_eyre::Report;
use confique::{toml, Config};
use owo_colors::OwoColorize;
use regex::Regex;
use rusqlite::Connection;
use serde::{de::Visitor, Deserialize, Serialize};
use slite::{
    read_sql_files,
    tui::{BroadcastWriter, ConfigHandler, Message, MigratorFactory, ReloadableConfig},
    Migrator, Options, SqlPrinter,
};
use std::{
    fs,
    path::{Path, PathBuf},
    str::FromStr,
};
use tokio::sync::mpsc;
use tracing::metadata::LevelFilter;
use tracing_subscriber::{
    prelude::*,
    reload::{self, Handle},
    util::SubscriberInitExt,
    Layer, Registry,
};
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
enum Command {
    Migrate { migrate: Migrate },
    Config { config: AppConfig },
    Diff,
    PrintSchema { from: SchemaType },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SerdeRegex(#[serde(with = "serde_regex")] Regex);

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

#[derive(Debug, Clone, Args, confique::Config, Serialize, Deserialize)]
pub struct Conf {
    #[arg(short, long, value_parser=source_parser)]
    pub source: Option<PathBuf>,
    #[arg(short, long, value_parser=destination_parser)]
    pub target: Option<PathBuf>,
    #[arg(short, long, value_parser=extension_parser)]
    pub extension: Option<Vec<PathBuf>>,
    #[arg(short, long, value_parser=regex_parser)]
    pub ignore: Option<SerdeRegex>,
    #[arg(short, long)]
    pub log_level: Option<SerdeLevel>,
}

#[derive(clap::Parser)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    #[command(flatten)]
    config: Conf,
}

fn source_parser(val: &str) -> Result<PathBuf, Report> {
    let path = PathBuf::from(val.to_owned());
    match path.try_exists() {
        Ok(true) => Ok(path),
        Ok(false) => Err(color_eyre::eyre::eyre!("Source path does not exist")),
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

fn extension_parser(val: &str) -> Result<PathBuf, Report> {
    let path = PathBuf::from(val.to_owned());
    match (path.try_exists(), path.is_file()) {
        (Ok(true), false) => Err(color_eyre::eyre::eyre!("Extension path must be a file")),
        (Ok(true), true) => Ok(path),
        (Ok(false), _) => Err(color_eyre::eyre::eyre!("Extension path does not exist")),
        (Err(e), _) => Err(color_eyre::eyre::eyre!("{e}")),
    }
}

fn regex_parser(val: &str) -> Result<SerdeRegex, regex::Error> {
    Ok(SerdeRegex(Regex::new(val)?))
}

pub struct ConfigStore {
    cli_config: Conf,
    tx: mpsc::Sender<Message>,
    reload_handle: Handle<LevelFilter, Registry>,
}

impl ConfigHandler<Conf> for ConfigStore {
    fn on_update(
        &mut self,
        previous_config: std::sync::Arc<Conf>,
        new_config: std::sync::Arc<Conf>,
    ) {
        if previous_config.source != new_config.source {
            self.tx
                .blocking_send(Message::SourceChanged(
                    new_config.source.clone().unwrap_or_default(),
                ))
                .unwrap();
        }
        if previous_config.target != new_config.target {
            self.tx
                .blocking_send(Message::TargetChanged(
                    new_config.target.clone().unwrap_or_default(),
                ))
                .unwrap();
        }
        if previous_config.log_level != new_config.log_level {
            self.reload_handle
                .modify(|l| {
                    *l = new_config
                        .log_level
                        .as_ref()
                        .unwrap_or(&SerdeLevel(LevelFilter::INFO))
                        .0
                })
                .unwrap();
        }
        if previous_config.extension != new_config.extension
            || previous_config.ignore != new_config.ignore
        {
            self.tx
                .blocking_send(Message::ConfigChanged(slite::Config {
                    extensions: new_config.extension.clone().unwrap_or_default(),
                    ignore: new_config.ignore.clone().map(|r| r.0),
                }))
                .unwrap();
        }
    }

    fn create_config(&self, path: &Path) -> Conf {
        let cli_config = self.cli_config.clone();
        let partial = confique_partial_conf::PartialConf {
            source: cli_config.source,
            target: cli_config.target,
            extension: cli_config.extension,
            ignore: cli_config.ignore,
            log_level: cli_config.log_level,
        };
        Conf::builder()
            .preloaded(partial)
            .file(path)
            .load()
            .unwrap()
    }
}

pub async fn run() -> Result<(), Report> {
    color_eyre::install()?;
    let cli = Cli::parse();
    let cli_config = cli.config.clone();
    let partial = confique_partial_conf::PartialConf {
        source: cli.config.source,
        target: cli.config.target,
        extension: cli.config.extension,
        ignore: cli.config.ignore,
        log_level: cli.config.log_level,
    };
    let conf = Conf::builder()
        .preloaded(partial)
        .file("slite.toml")
        .load()?;

    let source = conf.source.unwrap_or_default();
    let target = conf.target.unwrap_or_default();
    let extensions = conf.extension.unwrap_or_default();

    let ignore = conf.ignore.map(|i| i.0);
    let config = slite::Config { extensions, ignore };
    let log_level = conf.log_level.unwrap_or(SerdeLevel(LevelFilter::INFO));
    let schema = read_sql_files(&source);

    match cli.command {
        Some(Command::Migrate { migrate }) => match migrate {
            Migrate::Run => {
                let target_db = Connection::open(target)?;

                Registry::default()
                    .with(
                        HierarchicalLayer::default()
                            .with_indent_lines(true)
                            .with_level(false)
                            .with_filter(log_level.0),
                    )
                    .init();
                let migrator = Migrator::new(
                    &schema,
                    target_db,
                    config,
                    Options {
                        allow_deletions: true,
                        dry_run: false,
                    },
                )?;
                migrator.migrate()?;
            }
            Migrate::DryRun => {
                let target_db = Connection::open(target)?;
                Registry::default()
                    .with(
                        HierarchicalLayer::default()
                            .with_indent_lines(true)
                            .with_level(false)
                            .with_filter(log_level.0),
                    )
                    .init();
                let migrator = Migrator::new(
                    &schema,
                    target_db,
                    config,
                    Options {
                        allow_deletions: true,
                        dry_run: true,
                    },
                )?;
                migrator.migrate()?;
            }
            Migrate::Script => {
                let target_db = Connection::open(target)?;
                let migrator = Migrator::new(
                    &schema,
                    target_db,
                    config,
                    Options {
                        allow_deletions: true,
                        dry_run: true,
                    },
                )?;
                migrator.migrate_with_callback(|statement| println!("{statement}"))?;
            }
        },

        Some(Command::PrintSchema { from }) => {
            let source_db = Connection::open(target)?;
            let mut migrator = Migrator::new(
                &schema,
                source_db,
                config,
                Options {
                    allow_deletions: true,
                    dry_run: true,
                },
            )?;
            let mut sql_printer = SqlPrinter::default();
            let metadata = migrator.parse_metadata()?;
            let source = match from {
                SchemaType::Source => metadata.source,
                SchemaType::Target => metadata.target,
            };
            for (_, sql) in source.tables {
                println!("{}", sql_printer.print(&sql));
            }

            for (_, sql) in source.indexes {
                println!("{}", sql_printer.print(&sql));
            }
        }
        Some(Command::Diff) => {
            let target_db = Connection::open(target)?;
            let mut migrator = Migrator::new(
                &schema,
                target_db,
                config,
                Options {
                    allow_deletions: true,
                    dry_run: true,
                },
            )?;
            println!("{}", migrator.diff()?);
        }
        Some(Command::Config { config }) => {
            match config {
                AppConfig::Generate => match Path::new("slite.toml").try_exists() {
                    Ok(true) => println!("{}",
                    "Config file slite.toml already exists. Remove the file before regenerating."
                        .yellow()
                ),
                    Ok(false) => fs::write(
                        "slite.toml",
                        toml::template::<Conf>(toml::FormatOptions::default()),
                    )?,
                    Err(e) => println!("{}", format!("Error checking for config file: {e}").red()),
                },
            }
        }

        None => {
            let (filter, reload_handle) = reload::Layer::new(log_level.0);

            Registry::default()
                .with(
                    HierarchicalLayer::default()
                        .with_writer(BroadcastWriter::default())
                        .with_indent_lines(true)
                        .with_level(false)
                        .with_filter(filter),
                )
                .init();
            let (tx, rx) = mpsc::channel(32);
            let handler = ConfigStore {
                tx: tx.clone(),
                cli_config,
                reload_handle,
            };
            let _reloadable = ReloadableConfig::new(PathBuf::from("slite.toml"), handler);
            run_tui(MigratorFactory::new(source, target, config)?, tx, rx).await?;
        }
    }

    Ok(())
}
