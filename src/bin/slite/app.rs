use crate::app_tui::{self};
use clap::{ArgAction, Args, Parser, ValueEnum};
use color_eyre::Report;
use confique::{toml, Config};
use minus::Pager;
use notify_debouncer_mini::DebouncedEvent;
use owo_colors::OwoColorize;
use regex::Regex;
use rusqlite::Connection;
use serde::{de::Visitor, Deserialize, Serialize};
use slite::{
    error::InitializationError,
    read_sql_files,
    tui::{BroadcastWriter, ConfigHandler, Message, MigratorFactory, ReloadableConfig},
    Migrator, Options, SqlPrinter,
};
use std::{
    fmt::Write,
    fs, io,
    path::{Path, PathBuf},
    str::FromStr,
};
use tokio::sync::mpsc;
use tracing::metadata::LevelFilter;
use tracing_subscriber::{
    fmt::MakeWriter,
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
    Print { from: SchemaType },
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

struct PagerWrapper {
    pager: Pager,
}

impl io::Write for PagerWrapper {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        write!(self.pager, "{}", std::str::from_utf8(buf).unwrap()).unwrap();
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for PagerWrapper {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        Self {
            pager: self.pager.clone(),
        }
    }
}

#[derive(Debug, Clone, Args, confique::Config, Serialize, Deserialize)]
pub struct Conf {
    #[arg(short, long, value_parser = source_parser)]
    pub source: Option<PathBuf>,
    #[arg(short, long, value_parser = source_parser)]
    pub before_migration: Option<PathBuf>,
    #[arg(short, long, value_parser = source_parser)]
    pub after_migration: Option<PathBuf>,
    #[arg(short, long, value_parser = destination_parser)]
    pub target: Option<PathBuf>,
    #[arg(short, long, value_parser = extension_parser)]
    pub extension: Option<Vec<PathBuf>>,
    #[arg(short, long, value_parser = regex_parser)]
    pub ignore: Option<SerdeRegex>,
    #[arg(short, long)]
    pub log_level: Option<SerdeLevel>,
    #[arg(short, long, action = ArgAction::SetTrue)]
    pub pager: Option<bool>,
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
        events: Vec<DebouncedEvent>,
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
            || previous_config.before_migration != new_config.before_migration
            || previous_config.after_migration != new_config.after_migration
        {
            self.tx
                .blocking_send(Message::ConfigChanged(slite::Config {
                    extensions: new_config.extension.clone().unwrap_or_default(),
                    ignore: new_config.ignore.clone().map(|r| r.0),
                    before_migration: new_config
                        .before_migration
                        .clone()
                        .map(read_sql_files)
                        .unwrap_or_default(),
                    after_migration: new_config
                        .after_migration
                        .clone()
                        .map(read_sql_files)
                        .unwrap_or_default(),
                }))
                .unwrap();
        }

        if events.iter().any(|e| {
            new_config
                .before_migration
                .as_ref()
                .map(|p| e.path.starts_with(p))
                .unwrap_or(false)
                || new_config
                    .after_migration
                    .as_ref()
                    .map(|p| e.path.starts_with(p))
                    .unwrap_or(false)
        }) {
            self.tx
                .blocking_send(Message::ConfigChanged(slite::Config {
                    extensions: new_config.extension.clone().unwrap_or_default(),
                    ignore: new_config.ignore.clone().map(|r| r.0),
                    before_migration: new_config
                        .before_migration
                        .clone()
                        .map(read_sql_files)
                        .unwrap_or_default(),
                    after_migration: new_config
                        .after_migration
                        .clone()
                        .map(read_sql_files)
                        .unwrap_or_default(),
                }))
                .unwrap();
        }
    }

    fn create_config(&self, path: &Path) -> Conf {
        let cli_config = self.cli_config.clone();
        let partial = confique_partial_conf::PartialConf {
            source: cli_config.source,
            target: cli_config.target,
            before_migration: cli_config.before_migration,
            after_migration: cli_config.after_migration,
            extension: cli_config.extension,
            ignore: cli_config.ignore,
            log_level: cli_config.log_level,
            pager: cli_config.pager,
        };
        Conf::builder()
            .preloaded(partial)
            .file(path)
            .load()
            .unwrap()
    }

    fn watch_paths(&self, path: &Path) -> Vec<PathBuf> {
        let config = self.create_config(path);
        let mut paths = vec![path.to_path_buf()];
        if let Some(before) = config.before_migration {
            paths.push(before);
        }
        if let Some(after) = config.after_migration {
            paths.push(after);
        }
        paths
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
            extension: cli_config.extension,
            ignore: cli_config.ignore,
            log_level: cli_config.log_level,
            pager: cli_config.pager,
            before_migration: cli_config.before_migration,
            after_migration: cli_config.after_migration,
        };
        let conf = Conf::builder()
            .preloaded(partial)
            .file("slite.toml")
            .load()?;

        let source = conf.source.unwrap_or_default();
        let target = conf.target.unwrap_or_default();
        let extensions = conf.extension.unwrap_or_default();
        let ignore = conf.ignore.map(|i| i.0);
        let before_migration = conf
            .before_migration
            .map(read_sql_files)
            .unwrap_or_default();
        let after_migration = conf.after_migration.map(read_sql_files).unwrap_or_default();
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

    pub async fn run(mut self) -> Result<(), Report> {
        match self.cli.command.clone() {
            Some(command) => {
                let target_db = Connection::open(self.target.clone())?;

                match command {
                    Command::Migrate { migrate } => {
                        self.handle_migrate_command(&migrate, target_db)?;
                    }
                    Command::Print { from } => {
                        let migrator = self.get_migrator(
                            Options {
                                allow_deletions: true,
                                dry_run: true,
                            },
                            target_db,
                        )?;
                        self.print_schema(migrator, &from)?;
                    }
                    Command::Diff => {
                        let mut migrator = self.get_migrator(
                            Options {
                                allow_deletions: true,
                                dry_run: true,
                            },
                            target_db,
                        )?;
                        self.write(&migrator.diff()?)?;
                    }
                    Command::Config { config } => {
                        self.handle_config_command(&config)?;
                    }
                }
            }
            None => {
                self.run_tui().await?;
            }
        }
        Ok(())
    }

    fn init_logger(&mut self) {
        if let Some(pager) = self.pager.as_mut() {
            Registry::default()
                .with(
                    HierarchicalLayer::default()
                        .with_writer(PagerWrapper {
                            pager: pager.clone(),
                        })
                        .with_indent_lines(true)
                        .with_level(false)
                        .with_filter(self.log_level),
                )
                .init();
        } else {
            Registry::default()
                .with(
                    HierarchicalLayer::default()
                        .with_indent_lines(true)
                        .with_level(false)
                        .with_filter(self.log_level),
                )
                .init();
        }
    }

    fn write(&mut self, out: &str) -> Result<(), Report> {
        if let Some(pager) = self.pager.as_mut() {
            writeln!(pager, "{out}")?;
        } else {
            println!("{out}");
        }
        Ok(())
    }

    fn get_migrator(
        &self,
        options: Options,
        target_db: Connection,
    ) -> Result<Migrator, InitializationError> {
        Migrator::new(&self.schema, target_db, self.config.clone(), options)
    }

    fn handle_migrate_command(
        &mut self,
        migrate: &Migrate,
        target_db: Connection,
    ) -> Result<(), Report> {
        match migrate {
            Migrate::Run => {
                self.init_logger();
                self.get_migrator(
                    Options {
                        allow_deletions: true,
                        dry_run: false,
                    },
                    target_db,
                )?
                .migrate()?;
            }
            Migrate::DryRun => {
                self.init_logger();
                self.get_migrator(
                    Options {
                        allow_deletions: true,
                        dry_run: true,
                    },
                    target_db,
                )?
                .migrate()?;
            }
            Migrate::Script => {
                self.get_migrator(
                    Options {
                        allow_deletions: true,
                        dry_run: true,
                    },
                    target_db,
                )?
                .migrate_with_callback(|statement| self.write(&statement).unwrap())?;
            }
        }
        Ok(())
    }

    fn print_schema(&mut self, mut migrator: Migrator, from: &SchemaType) -> Result<(), Report> {
        let mut sql_printer = SqlPrinter::default();
        let metadata = migrator.parse_metadata()?;
        let source = match from {
            SchemaType::Source => metadata.source,
            SchemaType::Target => metadata.target,
        };
        for (_, sql) in source.tables {
            self.write(&sql_printer.print(&sql))?;
        }
        for (_, sql) in source.indexes {
            self.write(&sql_printer.print(&sql))?;
        }
        for (_, sql) in source.triggers {
            self.write(&sql_printer.print(&sql))?;
        }
        Ok(())
    }

    fn handle_config_command(&self, config: &AppConfig) -> Result<(), Report> {
        match config {
            AppConfig::Generate => match Path::new("slite.toml").try_exists() {
                Ok(true) => println!(
                    "{}",
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
        Ok(())
    }

    async fn run_tui(self) -> Result<(), Report> {
        let (filter, reload_handle) = reload::Layer::new(self.log_level);
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
            cli_config: self.cli_config,
            reload_handle,
        };
        let _reloadable = ReloadableConfig::new(PathBuf::from("slite.toml"), handler);
        app_tui::run_tui(
            MigratorFactory::new(self.source, self.target, self.config)?,
            tx,
            rx,
        )
        .await?;

        Ok(())
    }
}
