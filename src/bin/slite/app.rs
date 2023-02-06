use crate::app_tui::run_tui;
use clap::{Args, Parser, ValueEnum};
use color_eyre::Report;
use confique::{toml, Config};
use owo_colors::OwoColorize;
use regex::Regex;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use slite::{
    read_sql_files,
    tui::{BroadcastWriter, MigratorFactory},
    Migrator, Options, SqlPrinter,
};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tracing::{metadata::LevelFilter, Level};
use tracing_subscriber::{filter::Targets, prelude::*, util::SubscriberInitExt, Layer, Registry};
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

#[derive(Debug, Clone, Args, confique::Config, Serialize, Deserialize)]
struct Conf {
    #[arg(short, long, value_parser=source_parser)]
    source: Option<PathBuf>,
    #[arg(short, long, value_parser=destination_parser)]
    target: Option<PathBuf>,
    #[arg(short, long, value_parser=extension_parser)]
    extension: Option<Vec<PathBuf>>,
    #[arg(short, long, value_parser=regex_parser)]
    ignore: Option<SerdeRegex>,
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

pub async fn run() -> Result<(), Report> {
    color_eyre::install()?;
    let cli = Cli::parse();
    let partial = confique_partial_conf::PartialConf {
        source: cli.config.source,
        target: cli.config.target,
        extension: cli.config.extension,
        ignore: cli.config.ignore,
    };
    let conf = Conf::builder()
        .preloaded(partial)
        .file("slite.toml")
        .load()?;

    let source = conf.source.unwrap_or_default();
    let target = conf.target.unwrap_or_default();
    let extensions = conf.extension.unwrap_or_default();
    let ignore = conf.ignore.map(|i| i.0);
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
                            .with_filter(LevelFilter::TRACE),
                    )
                    .init();
                let migrator = Migrator::new(
                    &schema,
                    target_db,
                    Options {
                        allow_deletions: true,
                        dry_run: false,
                        extensions,
                        ignore,
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
                            .with_filter(LevelFilter::INFO),
                    )
                    .init();
                let migrator = Migrator::new(
                    &schema,
                    target_db,
                    Options {
                        allow_deletions: true,
                        dry_run: true,
                        extensions,
                        ignore,
                    },
                )?;
                migrator.migrate()?;
            }
            Migrate::Script => {
                let target_db = Connection::open(target)?;
                let migrator = Migrator::new(
                    &schema,
                    target_db,
                    Options {
                        allow_deletions: true,
                        dry_run: true,
                        extensions,
                        ignore,
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
                Options {
                    allow_deletions: true,
                    dry_run: true,
                    extensions,
                    ignore,
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
                Options {
                    allow_deletions: true,
                    dry_run: true,
                    extensions,
                    ignore,
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
            Registry::default()
                .with(
                    HierarchicalLayer::default()
                        .with_writer(BroadcastWriter::default())
                        .with_indent_lines(true)
                        .with_level(false)
                        .with_filter(Targets::default().with_target("slite", Level::TRACE)),
                )
                .init();

            run_tui(MigratorFactory::new(source, target, extensions, ignore)?).await?;
        }
    }

    Ok(())
}
