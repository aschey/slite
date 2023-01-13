#[cfg(feature = "pretty-print")]
mod ansi_sql_printer;

#[cfg(feature = "pretty-print")]
pub(crate) use ansi_sql_printer::SqlPrinter;

#[cfg(not(feature = "pretty-print"))]
mod default_sql_printer;

use connection::{PristineConnection, TargetConnection};
#[cfg(not(feature = "pretty-print"))]
pub(crate) use default_sql_printer::SqlPrinter;

mod connection;

use crate::connection::TargetTransaction;
use once_cell::sync::Lazy;
use regex::Regex;
use rusqlite::Connection;
use std::{cell::RefCell, collections::HashMap, fmt::Debug, rc::Rc};
use tracing::{debug, info, span, Level};

macro_rules! regex {
    ($name: ident, $re: literal $(,) ?) => {
        static $name: Lazy<Regex> = Lazy::new(|| Regex::new($re).expect("Regex should compile"));
    };
}

regex!(COMMENTS_RE, r"--[^\n]*\n");
regex!(WHITESPACE_RE, r"\s+");
regex!(EXTRA_WHITESPACE_RE, r" *([(),]) *");
regex!(QUOTES_RE, r#""(\w+)""#);

#[derive(thiserror::Error, Debug)]
pub enum InitializationError {
    #[error("{0}: {1}")]
    QueryFailure(String, QueryError),
    #[error("Failed to connect to the database {0}: {1}")]
    ConnectionFailure(String, #[source] rusqlite::Error),
}

#[derive(thiserror::Error, Debug)]
pub enum MigrationError {
    #[error("{0}: {1}")]
    QueryFailure(String, QueryError),
    #[error("Failed to initialize transaction: {0}")]
    TransactionInitializationFailure(#[source] rusqlite::Error),
    #[error("Failed to commit transaction: {0}")]
    TransactionCommitFailure(#[source] rusqlite::Error),
    #[error("Failed to rollback transaction: {0}")]
    TransactionRollbackFailure(#[source] rusqlite::Error),
    #[error("Aborting migration because data loss would occur and allow_deletions is false: {0}")]
    DataLoss(String),
    #[error("The following foreign keys have constraint violations: {0:?}")]
    ForeignKeyViolation(Vec<String>),
}

#[derive(thiserror::Error, Debug)]
#[error("Failed to execute query {0}: {1}")]
pub struct QueryError(String, #[source] rusqlite::Error);

pub type LogFn = Box<dyn Fn(&str)>;

pub struct Migrator {
    connection: Rc<RefCell<TargetConnection>>,
    pristine: PristineConnection,
    options: Options,
    foreign_keys_enabled: bool,
    modified: bool,
}

#[derive(Debug, Default)]
pub struct Options {
    pub allow_deletions: bool,
    pub dry_run: bool,
}

impl Migrator {
    pub fn new(
        connection: Connection,
        schema: &[impl AsRef<str>],
        options: Options,
    ) -> Result<Self, InitializationError> {
        let mut connection = TargetConnection::new(connection);
        let foreign_keys_enabled = connection.get_pragma::<i32>("foreign_keys").map_err(|e| {
            InitializationError::QueryFailure(
                "Failed to retrieve foreign_keys pragma".to_owned(),
                e,
            )
        })? == 1;
        if foreign_keys_enabled {
            connection
                .execute("PRAGMA foreign_keys = OFF")
                .map_err(|e| {
                    InitializationError::QueryFailure(
                        "Failed to disable foreign keys".to_owned(),
                        e,
                    )
                })?;
        }
        let mut pristine = PristineConnection::new()?;
        pristine.initialize_schema(schema)?;
        Ok(Self {
            connection: Rc::new(RefCell::new(connection)),
            foreign_keys_enabled,
            pristine,
            options,
            modified: false,
        })
    }

    pub fn migrate(mut self) -> Result<(), MigrationError> {
        let connection_rc = self.connection.clone();
        let mut connection = connection_rc.as_ref().borrow_mut();
        let mut tx = TargetTransaction::new(&mut connection)?;

        let migration_span = span!(Level::INFO, "Starting migration");
        let _migration_guard = migration_span.entered();
        let migrate_result = self.migrate_inner(&mut tx);
        match &migrate_result {
            Ok(()) => {
                tx.commit()?;
                if self.modified {
                    connection.vacuum().map_err(|e| {
                        MigrationError::QueryFailure("Failed to vacuum database".to_owned(), e)
                    })?;
                } else {
                    debug!("No changes detected, not optimizing database");
                }
            }
            Err(_) => {
                tx.rollback()?;
            }
        }
        if self.foreign_keys_enabled {
            connection
                .execute("PRAGMA foreign_keys = ON")
                .map_err(|e| {
                    MigrationError::QueryFailure("Failed to re-enable foreign keys".to_owned(), e)
                })?;
        }
        info!("Migration completed");
        migrate_result
    }

    fn migrate_inner(&mut self, tx: &mut TargetTransaction) -> Result<(), MigrationError> {
        if self.foreign_keys_enabled {
            tx.execute("PRAGMA defer_foreign_keys = TRUE")
                .map_err(|e| {
                    MigrationError::QueryFailure("Error enabling defer_foreign_keys".to_owned(), e)
                })?;
        }
        let table_span = span!(Level::INFO, "Migrating tables");
        let _table_guard = table_span.entered();
        let pristine_tables =
            self
                .pristine
                .select_metadata(
                    "SELECT name, sql from sqlite_master WHERE type = 'table' and name != 'sqlite_sequence'",
                )
                .map_err(
                    |e| MigrationError::QueryFailure("Failed to get tables from pristine database".to_owned(), e),
                )?;
        let tables =
            tx
                .select_metadata(
                    "SELECT name, sql from sqlite_master WHERE type = 'table' and name != 'sqlite_sequence'",
                )
                .map_err(
                    |e| MigrationError::QueryFailure("Failed to get tables from current database".to_owned(), e),
                )?;
        let new_tables: HashMap<&String, &String> = pristine_tables
            .iter()
            .filter(|(k, _)| !tables.contains_key(*k))
            .collect();
        let removed_tables: Vec<&String> = tables
            .keys()
            .filter(|k| !pristine_tables.contains_key(*k))
            .collect();

        if !removed_tables.is_empty() && !self.options.allow_deletions {
            let removed_table_list = removed_tables
                .into_iter()
                .map(|t| t.to_owned())
                .collect::<Vec<_>>()
                .join(",");
            return Err(MigrationError::DataLoss(format!(
                "The following tables would be removed: {removed_table_list}"
            )));
        }

        let empty = "".to_owned();
        let modified_tables: HashMap<&String, &String> = pristine_tables
            .iter()
            .filter(|(name, sql)| {
                normalize_sql(tables.get(*name).unwrap_or(&empty)) != normalize_sql(sql)
            })
            .collect();
        tx.execute("PRAGMA defer_foreign_keys = TRUE")
            .map_err(|e| {
                MigrationError::QueryFailure("Error enabling defer_foreign_keys".to_owned(), e)
            })?;

        let create_table_span = span!(Level::INFO, "Creating tables");
        let _create_table_guard = create_table_span.entered();
        if new_tables.is_empty() {
            info!("No tables to create");
        }
        for (new_table, new_table_sql) in new_tables {
            info!("Creating table {new_table}");
            tx.execute(new_table_sql).map_err(|e| {
                MigrationError::QueryFailure(format!("Error creating table {new_table}"), e)
            })?;
        }
        drop(_create_table_guard);

        let drop_table_span = span!(Level::INFO, "Dropping tables");
        let _drop_table_guard = drop_table_span.entered();
        if removed_tables.is_empty() {
            info!("No tables to drop");
        }
        for removed_table in removed_tables {
            info!("Dropping table {removed_table}");
            tx.execute(&format!("DROP TABLE {removed_table}"))
                .map_err(|e| {
                    MigrationError::QueryFailure(format!("Error dropping table {removed_table}"), e)
                })?;
        }
        drop(_drop_table_guard);

        let modify_table_span = span!(Level::INFO, "Modifying tables");
        let _modify_table_guard = modify_table_span.entered();
        if modified_tables.is_empty() {
            info!("No tables to modify");
        }
        for (modified_table, modified_table_sql) in modified_tables {
            info!("Modifying table {modified_table}");
            let temp_table = format!("{modified_table}_migration_new");
            let create_table_regex = Regex::new(&format!(r"\b{}\b", regex::escape(modified_table)))
                .expect("Regex should compile");
            let create_temp_table_sql =
                create_table_regex.replace_all(modified_table_sql, &temp_table);
            tx.execute(&create_temp_table_sql).map_err(|e| {
                MigrationError::QueryFailure(format!("Error creating temp table {temp_table}"), e)
            })?;
            let cols = tx.get_cols(modified_table).map_err(|e| {
                MigrationError::QueryFailure(
                    format!("Error getting columns for table {modified_table}"),
                    e,
                )
            })?;
            let pristine_cols = self.pristine.get_cols(modified_table).map_err(|e| {
                MigrationError::QueryFailure(
                    format!("Error getting columns for table {modified_table}"),
                    e,
                )
            })?;
            let removed_cols: Vec<&String> =
                cols.iter().filter(|c| !pristine_cols.contains(c)).collect();
            if !self.options.allow_deletions && !removed_cols.is_empty() {
                return Err(MigrationError::DataLoss(format!(
                    "The following columns would be dropped: {}",
                    removed_cols
                        .into_iter()
                        .map(|c| c.to_owned())
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
            let common_cols = cols
                .into_iter()
                .filter(|c| pristine_cols.contains(c))
                .collect::<Vec<_>>()
                .join(",");
            tx
                .execute(
                    &format!("INSERT INTO {temp_table} ({common_cols}) SELECT {common_cols} FROM {modified_table}"),
                )
                .map_err(|e| {
                    MigrationError::QueryFailure(format!("Error migrating data into table {modified_table}"), e)
                })?;
            tx.execute(&format!("DROP TABLE {modified_table}"))
                .map_err(|e| {
                    MigrationError::QueryFailure(
                        format!("Error dropping table {modified_table}"),
                        e,
                    )
                })?;
            tx.execute(&format!(
                "ALTER TABLE {temp_table} RENAME TO {modified_table}"
            ))
            .map_err(|e| {
                MigrationError::QueryFailure(
                    format!("Error renaming {temp_table} to {modified_table}"),
                    e,
                )
            })?;
        }
        drop(_modify_table_guard);
        drop(_table_guard);

        let index_span = span!(Level::INFO, "Migrating indexes");
        let _index_guard = index_span.entered();
        let pristine_indexes = self
            .pristine
            .select_metadata("SELECT name, sql FROM sqlite_master WHERE type = 'index'")
            .map_err(|e| {
                MigrationError::QueryFailure(
                    "Failed to get indexes from pristine database".to_owned(),
                    e,
                )
            })?;
        let indexes = tx
            .select_metadata("SELECT name, sql FROM sqlite_master WHERE type = 'index'")
            .map_err(|e| {
                MigrationError::QueryFailure(
                    "Failed to get indexes from current database".to_owned(),
                    e,
                )
            })?;
        let old_indexes = indexes
            .keys()
            .filter(|k| !pristine_indexes.contains_key(*k));
        for index in old_indexes {
            info!("Dropping index {index}");
            tx.execute(&format!("DROP INDEX {index}")).map_err(|e| {
                MigrationError::QueryFailure(format!("Failed to drop index {index}"), e)
            })?;
        }
        for (index_name, sql) in pristine_indexes {
            match indexes.get(&index_name) {
                Some(old_index) if normalize_sql(&sql) != normalize_sql(old_index) => {
                    info!("Updating index {index_name}");
                    tx.execute(&format!("DROP INDEX {index_name}"))
                        .map_err(|e| {
                            MigrationError::QueryFailure(
                                format!("Error dropping index {index_name}"),
                                e,
                            )
                        })?;
                    tx.execute(&sql).map_err(|e| {
                        MigrationError::QueryFailure(
                            format!("Error creating index {index_name}"),
                            e,
                        )
                    })?;
                }
                None => {
                    info!("Creating index {index_name}");
                    tx.execute(&sql).map_err(|e| {
                        MigrationError::QueryFailure(
                            format!("Error creating index {index_name}"),
                            e,
                        )
                    })?;
                }
                _ => {}
            }
        }
        drop(_index_guard);
        if self
            .pristine
            .get_pragma::<i32>("foreign_keys")
            .map_err(|e| {
                MigrationError::QueryFailure(
                    "Failed to get foreign_keys pragma from pristine database".to_owned(),
                    e,
                )
            })?
            == 1
        {
            let foreign_key_violations: Vec<String> = tx
                .query("PRAGMA foreign_key_check", |row| row.get(0))
                .map_err(|e| {
                    MigrationError::QueryFailure("Error executing foreign key check".to_owned(), e)
                })?;
            if !foreign_key_violations.is_empty() {
                return Err(MigrationError::ForeignKeyViolation(foreign_key_violations));
            }
        }
        Ok(())
    }
}

fn normalize_sql(sql: &str) -> String {
    let sql = COMMENTS_RE.replace_all(sql, "");
    let sql = WHITESPACE_RE.replace_all(&sql, " ");
    let sql = EXTRA_WHITESPACE_RE.replace_all(&sql, r"$1");
    let sql = QUOTES_RE.replace_all(&sql, r"$1");
    sql.trim().to_owned()
}
#[cfg(test)]
#[path = "./lib_test.rs"]
mod lib_test;
