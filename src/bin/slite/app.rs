use std::fmt::Write;
use std::fs;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use clap::{ArgAction, Args, CommandFactory, Parser, ValueEnum};
use clap_complete::{Shell, generate};
use color_eyre::Report;
use confique::{Config, toml};
use elm_ui::{Command, Message};
use minus::Pager;
use normpath::PathExt;
use notify_debouncer_mini::DebouncedEvent;
use owo_colors::OwoColorize;
use regex::Regex;
use rusqlite::Connection;
use serde::de::Visitor;
use serde::{Deserialize, Serialize};
use slite::error::InitializationError;
use slite::tui::{AppMessage, BroadcastWriter, ConfigHandler, MigratorFactory};
use slite::{Migrator, Options, SqlPrinter, read_extension_dir, read_sql_files};
use tokio::sync::mpsc;
use tracing::metadata::LevelFilter;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::prelude::*;
use tracing_subscriber::reload::{self, Handle};
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{Layer, Registry};
use tracing_tree2::HierarchicalLayer;

use crate::app_tui::{self, TuiAppMessage};

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

pub struct ConfigStore {
    cli_config: Conf,
    tx: mpsc::Sender<elm_ui::Command>,
    reload_handle: Handle<Targets, Registry>,
}

impl ConfigStore {
    pub fn new(
        cli_config: Conf,
        tx: mpsc::Sender<elm_ui::Command>,
        reload_handle: Handle<Targets, Registry>,
    ) -> Self {
        Self {
            cli_config,
            tx,
            reload_handle,
        }
    }
}

impl ConfigHandler<Conf> for ConfigStore {
    fn on_update(
        &mut self,
        previous_config: Arc<Conf>,
        new_config: Arc<Conf>,
        events: Vec<DebouncedEvent>,
    ) -> Result<(), mpsc::error::SendError<Command>> {
        if previous_config.source != new_config.source {
            self.tx.blocking_send(Command::simple(Message::custom(
                TuiAppMessage::SourceChanged(
                    previous_config.source.clone().unwrap_or_default(),
                    new_config.source.clone().unwrap_or_default(),
                ),
            )))?;
        }

        if previous_config.target != new_config.target {
            self.tx.blocking_send(Command::simple(Message::custom(
                TuiAppMessage::TargetChanged(
                    previous_config.target.clone().unwrap_or_default(),
                    new_config.target.clone().unwrap_or_default(),
                ),
            )))?;
        }

        if previous_config.log_level != new_config.log_level {
            self.update_log_level(&new_config.log_level);
        }

        if previous_config.pre_migration != new_config.pre_migration {
            self.tx.blocking_send(Command::simple(Message::custom(
                TuiAppMessage::PathChanged(
                    previous_config.pre_migration.clone(),
                    new_config.pre_migration.clone(),
                ),
            )))?;
        }

        if previous_config.post_migration != new_config.post_migration {
            self.tx.blocking_send(Command::simple(Message::custom(
                TuiAppMessage::PathChanged(
                    previous_config.post_migration.clone(),
                    new_config.post_migration.clone(),
                ),
            )))?;
        }

        if previous_config.migrator_config_changed(&new_config) {
            self.send_config_changed(&new_config)?;
        }

        if self.contains_path(&events, &new_config.source) {
            self.tx
                .blocking_send(elm_ui::Command::simple(Message::custom(
                    AppMessage::FileChanged,
                )))?;
        }

        if self.contains_path(&events, &new_config.pre_migration)
            || self.contains_path(&events, &new_config.post_migration)
        {
            self.send_config_changed(&new_config)?;
        }

        Ok(())
    }

    fn create_config(&self, path: &Path) -> Conf {
        let cli_config = self.cli_config.clone();
        let partial = confique_partial_conf::PartialConf {
            source: cli_config.source,
            target: cli_config.target,
            pre_migration: cli_config.pre_migration,
            post_migration: cli_config.post_migration,
            extension_dir: cli_config.extension_dir,
            ignore: cli_config.ignore,
            log_level: cli_config.log_level,
            pager: cli_config.pager,
        };
        Conf::builder()
            .preloaded(partial)
            .file(path)
            .env()
            .load()
            .unwrap()
    }

    fn watch_paths(&self, path: &Path) -> Vec<PathBuf> {
        let config = self.create_config(path);
        let mut paths = vec![path.to_path_buf()];
        if let Some(source) = config.source {
            paths.push(source);
        }
        if let Some(before) = config.pre_migration {
            paths.push(before);
        }
        if let Some(after) = config.post_migration {
            paths.push(after);
        }
        paths
    }
}

impl ConfigStore {
    fn contains_path(&self, events: &[DebouncedEvent], search: &Option<PathBuf>) -> bool {
        events.iter().any(|e| {
            search
                .as_ref()
                .map(|p| {
                    if let Ok(e_norm) = e.path.normalize()
                        && let Ok(p_norm) = p.normalize()
                    {
                        e_norm.starts_with(p_norm)
                    } else {
                        false
                    }
                })
                .unwrap_or(false)
        })
    }

    fn send_config_changed(
        &self,
        new_config: &Arc<Conf>,
    ) -> Result<(), mpsc::error::SendError<Command>> {
        self.tx
            .blocking_send(Command::simple(Message::custom(AppMessage::ConfigChanged(
                slite::Config {
                    extensions: new_config
                        .extension_dir
                        .clone()
                        .map(read_extension_dir)
                        .unwrap()
                        .unwrap_or_default(),
                    ignore: new_config.ignore.clone().map(|r| r.0),
                    before_migration: new_config
                        .pre_migration
                        .clone()
                        .map(read_sql_files)
                        .unwrap_or_default(),
                    after_migration: new_config
                        .post_migration
                        .clone()
                        .map(read_sql_files)
                        .unwrap_or_default(),
                },
            ))))
    }

    fn update_log_level(&self, log_level: &Option<SerdeLevel>) {
        self.reload_handle
            .modify(|l| {
                *l = Targets::default().with_target(
                    "slite",
                    log_level
                        .as_ref()
                        .unwrap_or(&SerdeLevel(LevelFilter::INFO))
                        .0,
                )
            })
            .unwrap();
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
        owo_colors::set_override(io::stdout().is_terminal());
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
                Path::new("."),
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
            && io::stdout().is_terminal()
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
            Some(AppCommand::Completions { shell }) => {
                generate(
                    shell,
                    &mut Cli::command(),
                    Cli::command().get_name().to_string(),
                    &mut io::stdout(),
                );
            }
            Some(command) => {
                let target_db = Connection::open(self.target.clone())?;

                match command {
                    AppCommand::Migrate { migrate } => {
                        self.handle_migrate_command(&migrate, target_db)?;
                    }
                    AppCommand::Print { from } => {
                        let migrator = self.get_migrator(
                            Options {
                                allow_deletions: true,
                                dry_run: true,
                            },
                            target_db,
                        )?;
                        self.print_schema(migrator, &from)?;
                    }
                    AppCommand::Diff => {
                        let mut migrator = self.get_migrator(
                            Options {
                                allow_deletions: true,
                                dry_run: true,
                            },
                            target_db,
                        )?;
                        self.write(&migrator.diff()?)?;
                    }
                    AppCommand::Config { config } => {
                        self.handle_config_command(&config)?;
                    }
                    _ => {}
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
        for object in source.all_objects() {
            self.write(&sql_printer.print(&object.sql))?;
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
        BroadcastWriter::disable();

        let (filter, reload_handle) =
            reload::Layer::new(Targets::default().with_target("slite", self.log_level));
        Registry::default()
            .with(
                HierarchicalLayer::default()
                    .with_writer(BroadcastWriter::default())
                    .with_indent_lines(true)
                    .with_level(false)
                    .with_filter(filter),
            )
            .init();

        app_tui::run_tui(
            MigratorFactory::new(self.source, self.target, self.config)?,
            self.cli_config,
            reload_handle,
        )
        .await?;

        Ok(())
    }
}
