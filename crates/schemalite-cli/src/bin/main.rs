use clap::{Parser, ValueEnum};
use color_eyre::Report;
use rusqlite::{Connection, OpenFlags};
use schemalite::{tui::BroadcastWriter, Migrator, Options, SqlPrinter};
use schemalite_cli::run_tui;
use tracing::{metadata::LevelFilter, Level};
use tracing_subscriber::{filter::Targets, prelude::*, util::SubscriberInitExt, Layer, Registry};
use tracing_tree::HierarchicalLayer;

#[derive(ValueEnum, Clone)]
enum SchemaType {
    Source,
    Target,
}

#[derive(clap::Subcommand, Clone)]
enum Command {
    Migrate,
    DryRun,
    Diff,
    Generate,
    PrintSchema { from: SchemaType },
}

#[derive(clap::Parser)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[tokio::main]
async fn main() -> Result<(), Report> {
    color_eyre::install()?;
    let cli = Cli::parse();
    let source_db = Connection::open_with_flags(
        "file:memdb123",
        OpenFlags::default() | OpenFlags::SQLITE_OPEN_MEMORY | OpenFlags::SQLITE_OPEN_SHARED_CACHE,
    )?;

    source_db.execute_batch(schemas()[1])?;

    match cli.command {
        Some(Command::Migrate) => {
            Registry::default()
                .with(
                    HierarchicalLayer::default()
                        .with_indent_lines(true)
                        .with_level(false)
                        .with_filter(LevelFilter::TRACE),
                )
                .init();
            let migrator = Migrator::new(
                source_db,
                &[schemas()[2]],
                Options {
                    allow_deletions: true,
                    dry_run: false,
                },
            )?;
            migrator.migrate()?;
        }
        Some(Command::DryRun) => {
            Registry::default()
                .with(
                    HierarchicalLayer::default()
                        .with_indent_lines(true)
                        .with_level(false)
                        .with_filter(LevelFilter::INFO),
                )
                .init();
            let migrator = Migrator::new(
                source_db,
                &[schemas()[2]],
                Options {
                    allow_deletions: true,
                    dry_run: true,
                },
            )?;
            migrator.migrate()?;
        }
        Some(Command::PrintSchema { from }) => {
            let mut migrator = Migrator::new(
                source_db,
                &[schemas()[2]],
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
            let mut migrator = Migrator::new(
                source_db,
                &[schemas()[2]],
                Options {
                    allow_deletions: true,
                    dry_run: true,
                },
            )?;
            println!("{}", migrator.diff()?);
        }
        Some(Command::Generate) => {
            let migrator = Migrator::new(
                source_db,
                &[schemas()[2]],
                Options {
                    allow_deletions: true,
                    dry_run: true,
                },
            )?;
            let script = migrator.migrate()?;
            println!("{}", script.join("\n"));
        }
        None => {
            Registry::default()
                .with(
                    HierarchicalLayer::default()
                        .with_writer(BroadcastWriter::default())
                        .with_indent_lines(true)
                        .with_level(false)
                        .with_filter(Targets::default().with_target("schemalite", Level::TRACE)),
                )
                .init();

            let mut migrator = Migrator::new(
                source_db,
                &[schemas()[2]],
                Options {
                    allow_deletions: true,
                    dry_run: true,
                },
            )?;
            run_tui(migrator.parse_metadata()?, |options| {
                Migrator::new(
                    Connection::open_with_flags(
                        "file:memdb123",
                        OpenFlags::default()
                            | OpenFlags::SQLITE_OPEN_MEMORY
                            | OpenFlags::SQLITE_OPEN_SHARED_CACHE,
                    )
                    .unwrap(),
                    &[schemas()[2]],
                    options,
                )
                .unwrap()
            })
            .await?;
        }
    }

    Ok(())
}

fn schemas() -> [&'static str; 6] {
    [
        // 0
        "",
        // 1
        r#"
        PRAGMA foreign_keys = OFF;

        CREATE TABLE Node(
            node_oid INTEGER PRIMARY KEY NOT NULL,
            node_id INTEGER NOT NULL);
        CREATE UNIQUE INDEX Node_node_id on Node(node_id);
        "#,
        // 2
        // Added Node.active
        // Changed node_id type from INTEGER to TEXT
        // New table Job
        r#"
        PRAGMA foreign_keys = ON;
    
        CREATE TABLE Node(
            node_oid INTEGER PRIMARY KEY NOT NULL,
            node_id TEXT NOT NULL,
            active BOOLEAN NOT NULL DEFAULT(1),
            something_else TEXT);
        CREATE UNIQUE INDEX Node_node_id on Node(node_id);
    
        CREATE TABLE Job(
            node_oid INTEGER NOT NULL,
            id INTEGER NOT NULL,
            FOREIGN KEY(node_oid) REFERENCES Node(node_oid));
        CREATE UNIQUE INDEX Job_node_oid on Job(node_oid, id);
        "#,
        // 3
        // Remove field something_else.  Note: this is significant because
        // Job.node_oid references table Node which must be recreated.
        r#"
        PRAGMA foreign_keys = ON;
    
        CREATE TABLE Node(
            node_oid INTEGER PRIMARY KEY NOT NULL,
            node_id TEXT NOT NULL,
            active BOOLEAN NOT NULL DEFAULT(1));
        CREATE UNIQUE INDEX Node_node_id on Node(node_id);
    
        CREATE TABLE Job(
            node_oid INTEGER NOT NULL,
            id INTEGER NOT NULL,
            FOREIGN KEY(node_oid) REFERENCES Node(node_oid));
        CREATE UNIQUE INDEX Job_node_oid on Job(node_oid, id);
        "#,
        // 4
        // Change index Node_node_id field
        // Delete index Job_node_id
        // Set user_version = 6
        r#"
        PRAGMA foreign_keys = ON;
    
        CREATE TABLE Node(
            node_oid INTEGER PRIMARY KEY NOT NULL,
            node_id TEXT NOT NULL,
            active BOOLEAN NOT NULL DEFAULT(1));
        CREATE UNIQUE INDEX Node_node_id on Node(node_oid);
    
        CREATE TABLE Job(
            node_oid INTEGER NOT NULL,
            id INTEGER NOT NULL,
            FOREIGN KEY(node_oid) REFERENCES Node(node_oid));
        CREATE UNIQUE INDEX Job_node_oid on Job(node_oid, id);
    
        PRAGMA user_version = 6;
        "#,
        // 5
        // (vs. schema[1]) - Change Node.active default from 1 to 2
        r#"
        CREATE TABLE Node(
            node_oid INTEGER PRIMARY KEY NOT NULL,
            node_id TEXT NOT NULL,
            active BOOLEAN NOT NULL DEFAULT(2));
        CREATE UNIQUE INDEX Node_node_id on Node(node_id);
        "#,
    ]
}
