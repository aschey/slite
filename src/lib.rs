#[cfg(feature = "pretty-print")]
mod ansi_sql_printer;
#[cfg(feature = "pretty-print")]
pub use ansi_sql_printer::SqlPrinter;
#[cfg(not(feature = "pretty-print"))]
mod default_sql_printer;
#[cfg(feature = "diff")]
mod diff;
#[cfg(feature = "diff")]
mod unified_diff_builder;
#[cfg(feature = "diff")]
pub use diff::*;
mod color;
#[cfg(feature = "tui")]
pub mod tui;
pub use color::*;
mod connection;
pub use connection::*;
pub mod error;

use crate::connection::TargetTransaction;
#[cfg(not(feature = "pretty-print"))]
pub use default_sql_printer::SqlPrinter;
use error::{InitializationError, MigrationError, QueryError};
use once_cell::sync::Lazy;
use regex::Regex;
use rusqlite::Connection;
use std::{
    collections::HashMap,
    fmt::Debug,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tracing::{debug, info, span, Level};

macro_rules! regex {
    ($name: ident, $re: literal $(,) ?) => {
        static $name: Lazy<Regex> = Lazy::new(|| Regex::new($re).expect("Regex failed to compile"));
    };
}

regex!(COMMENTS_RE, r"--[^\n]*\n");
regex!(WHITESPACE_RE, r"\s+");
regex!(EXTRA_WHITESPACE_RE, r" *([(),]) *");
regex!(QUOTES_RE, r#""(\w+)""#);

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub allow_deletions: bool,
    pub dry_run: bool,
}

#[derive(Debug, Default, Clone)]
pub struct Config {
    pub extensions: Vec<PathBuf>,
    pub ignore: Option<Regex>,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct Settings {
    pub(crate) options: Options,
    pub(crate) config: Config,
}

pub struct Migrator {
    target_connection: Arc<Mutex<TargetConnection>>,
    pristine: PristineConnection,
    settings: Settings,
    foreign_keys_enabled: bool,
}

#[cfg(feature = "read-files")]
pub fn read_sql_files(sql_dir: impl AsRef<std::path::Path>) -> Vec<String> {
    let mut paths: Vec<_> = ignore::WalkBuilder::new(sql_dir)
        .max_depth(Some(5))
        .filter_entry(|entry| {
            let path = entry.path();
            path.is_dir() || path.extension().map(|e| e == "sql").unwrap_or(false)
        })
        .build()
        .filter_map(|dir_result| dir_result.ok().map(|d| d.path().to_path_buf()))
        .collect();

    paths.sort_by(|a, b| {
        let a_seq = get_sequence(a);
        let b_seq = get_sequence(b);
        a_seq.cmp(&b_seq)
    });
    paths
        .iter()
        .filter(|p| p.is_file())
        .map(|p| std::fs::read_to_string(p).unwrap())
        .collect()
}

fn get_sequence(path: &Path) -> i32 {
    let path_str = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let seq = path_str.split('-').next();
    if let Some(first) = seq {
        if let Ok(seq_num) = first.parse::<i32>() {
            return seq_num;
        }
    }
    i32::MIN
}

impl Migrator {
    pub fn new(
        schema: &[impl AsRef<str>],
        target_connection: Connection,
        config: Config,
        options: Options,
    ) -> Result<Self, InitializationError> {
        let settings = Settings { config, options };
        let mut target_connection = TargetConnection::new(target_connection, settings.clone());
        let foreign_keys_enabled = target_connection
            .get_pragma::<i32>("foreign_keys")
            .map_err(|e| {
                InitializationError::QueryFailure(
                    "Failed to retrieve foreign_keys pragma".to_owned(),
                    e,
                )
            })?
            == 1;
        if foreign_keys_enabled {
            target_connection
                .execute("PRAGMA foreign_keys = OFF")
                .map_err(|e| {
                    InitializationError::QueryFailure(
                        "Failed to disable foreign keys".to_owned(),
                        e,
                    )
                })?;
        }
        let mut pristine = PristineConnection::new(settings.clone())?;
        pristine.initialize_schema(schema)?;
        Ok(Self {
            target_connection: Arc::new(Mutex::new(target_connection)),
            foreign_keys_enabled,
            pristine,
            settings,
        })
    }

    pub fn migrate(self) -> Result<(), MigrationError> {
        self.migrate_with_callback(|_| {})
    }

    pub fn migrate_with_callback(
        mut self,
        on_script: impl Fn(String),
    ) -> Result<(), MigrationError> {
        let connection_rc = self.target_connection.clone();
        let mut connection = connection_rc.lock().expect("Failed to lock mutex");
        let mut tx = TargetTransaction::new(&mut connection, self.settings.clone(), on_script)?;

        let migration_span = span!(Level::INFO, "Starting migration");
        let _migration_guard = migration_span.entered();
        let migrate_result = self.migrate_inner(&mut tx);
        let result = match migrate_result {
            Ok(()) => {
                let modified = tx.modified();
                tx.commit()?;
                if modified {
                    connection.vacuum().map_err(|e| {
                        MigrationError::QueryFailure("Failed to vacuum database".to_owned(), e)
                    })?;
                } else {
                    debug!("No changes detected, not optimizing database");
                }
                Ok(())
            }
            Err(e) => {
                tx.rollback()?;
                Err(e)
            }
        };
        if self.foreign_keys_enabled {
            connection
                .execute("PRAGMA foreign_keys = ON")
                .map_err(|e| {
                    MigrationError::QueryFailure("Failed to re-enable foreign keys".to_owned(), e)
                })?;
        }
        info!("Migration completed");
        result
    }

    fn migrate_inner<F>(&mut self, tx: &mut TargetTransaction<F>) -> Result<(), MigrationError>
    where
        F: Fn(String),
    {
        if self.foreign_keys_enabled {
            tx.execute("PRAGMA defer_foreign_keys = TRUE")
                .map_err(|e| {
                    MigrationError::QueryFailure("Error enabling defer_foreign_keys".to_owned(), e)
                })?;
        }

        let pristine_metadata = self.pristine.parse_metadata().map_err(|e| {
            MigrationError::QueryFailure(
                "Failed to get metadata from pristine database".to_owned(),
                e,
            )
        })?;

        self.migrate_tables(tx, &pristine_metadata)?;
        self.migrate_indexes(tx, &pristine_metadata)?;

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

    fn migrate_tables<F>(
        &mut self,
        tx: &mut TargetTransaction<F>,
        pristine_metadata: &Metadata,
    ) -> Result<(), MigrationError>
    where
        F: Fn(String),
    {
        let table_span = span!(Level::INFO, "Migrating tables");
        let _table_guard = table_span.entered();

        let metadata = tx.parse_metadata().map_err(|e| {
            MigrationError::QueryFailure(
                "Failed to get metadata from current database".to_owned(),
                e,
            )
        })?;

        self.create_new_tables(tx, pristine_metadata, &metadata)?;
        self.drop_old_tables(tx, pristine_metadata, &metadata)?;
        self.update_tables(tx, pristine_metadata, &metadata)?;

        Ok(())
    }

    fn create_new_tables<F>(
        &mut self,
        tx: &mut TargetTransaction<F>,
        pristine_metadata: &Metadata,
        metadata: &Metadata,
    ) -> Result<(), MigrationError>
    where
        F: Fn(String),
    {
        let create_table_span = span!(Level::INFO, "Creating tables");
        let _create_table_guard = create_table_span.entered();

        let new_tables: HashMap<&String, &String> = pristine_metadata
            .tables
            .iter()
            .filter(|(k, _)| !metadata.tables.contains_key(*k))
            .collect();

        if new_tables.is_empty() {
            info!("No tables to create");
        }
        for (new_table, new_table_sql) in new_tables {
            info!("Creating table {new_table}");
            tx.execute(new_table_sql).map_err(|e| {
                MigrationError::QueryFailure(format!("Error creating table {new_table}"), e)
            })?;
        }
        Ok(())
    }

    fn drop_old_tables<F>(
        &mut self,
        tx: &mut TargetTransaction<F>,
        pristine_metadata: &Metadata,
        metadata: &Metadata,
    ) -> Result<(), MigrationError>
    where
        F: Fn(String),
    {
        let drop_table_span = span!(Level::INFO, "Dropping tables");
        let _drop_table_guard = drop_table_span.entered();

        let removed_tables: Vec<&String> = metadata
            .tables
            .keys()
            .filter(|k| !pristine_metadata.tables.contains_key(*k))
            .collect();

        if !removed_tables.is_empty() && !self.settings.options.allow_deletions {
            let removed_table_list = removed_tables
                .into_iter()
                .map(|t| t.to_owned())
                .collect::<Vec<_>>()
                .join(",");
            return Err(MigrationError::DataLoss(format!(
                "The following tables would be removed: {removed_table_list}"
            )));
        }

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
        Ok(())
    }

    fn update_tables<F>(
        &mut self,
        tx: &mut TargetTransaction<F>,
        pristine_metadata: &Metadata,
        metadata: &Metadata,
    ) -> Result<(), MigrationError>
    where
        F: Fn(String),
    {
        let modify_table_span = span!(Level::INFO, "Modifying tables");
        let _modify_table_guard = modify_table_span.entered();

        let modified_tables: HashMap<&String, &String> = pristine_metadata
            .tables
            .iter()
            .filter(|(name, sql)| {
                if let Some(existing) = metadata.tables.get(*name) {
                    normalize_sql(existing) != normalize_sql(sql)
                } else {
                    false
                }
            })
            .collect();

        if modified_tables.is_empty() {
            info!("No tables to modify");
        }
        for (modified_table, modified_table_sql) in modified_tables {
            self.update_table(tx, modified_table, modified_table_sql)?;
        }
        Ok(())
    }

    fn update_table<F>(
        &mut self,
        tx: &mut TargetTransaction<F>,
        modified_table: &str,
        modified_table_sql: &str,
    ) -> Result<(), MigrationError>
    where
        F: Fn(String),
    {
        info!("Modifying table {modified_table}");
        let temp_table = format!("{modified_table}_migration_new");
        let create_table_regex = Regex::new(&format!(r"\b{}\b", regex::escape(modified_table)))
            .expect("Regex failed to compile");
        let create_temp_table_sql = create_table_regex.replace_all(modified_table_sql, &temp_table);
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
        if !self.settings.options.allow_deletions && !removed_cols.is_empty() {
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
        tx.execute(&format!(
            "INSERT INTO {temp_table} ({common_cols}) SELECT {common_cols} FROM {modified_table}"
        ))
        .map_err(|e| {
            MigrationError::QueryFailure(
                format!("Error migrating data into table {modified_table}"),
                e,
            )
        })?;
        tx.execute(&format!("DROP TABLE {modified_table}"))
            .map_err(|e| {
                MigrationError::QueryFailure(format!("Error dropping table {modified_table}"), e)
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
        Ok(())
    }

    fn migrate_indexes<F>(
        &mut self,
        tx: &mut TargetTransaction<F>,
        pristine_metadata: &Metadata,
    ) -> Result<(), MigrationError>
    where
        F: Fn(String),
    {
        let index_span = span!(Level::INFO, "Migrating indexes");
        let _index_guard = index_span.entered();

        let metadata = tx.parse_metadata().map_err(|e| {
            MigrationError::QueryFailure(
                "Failed to get metadata from current database".to_owned(),
                e,
            )
        })?;

        let old_indexes: Vec<_> = metadata
            .indexes
            .keys()
            .filter(|k| !pristine_metadata.indexes.contains_key(*k))
            .collect();

        if old_indexes.is_empty() {
            info!("No indexes to drop");
        }

        for index in old_indexes {
            info!("Dropping index {index}");
            tx.execute(&format!("DROP INDEX {index}")).map_err(|e| {
                MigrationError::QueryFailure(format!("Failed to drop index {index}"), e)
            })?;
        }
        let mut index_updated = false;
        let mut index_created = false;
        for (index_name, sql) in &pristine_metadata.indexes {
            match metadata.indexes.get(index_name) {
                Some(old_index) if normalize_sql(sql) != normalize_sql(old_index) => {
                    index_updated = true;
                    info!("Updating index {index_name}");
                    tx.execute(&format!("DROP INDEX {index_name}"))
                        .map_err(|e| {
                            MigrationError::QueryFailure(
                                format!("Error dropping index {index_name}"),
                                e,
                            )
                        })?;
                    tx.execute(sql).map_err(|e| {
                        MigrationError::QueryFailure(
                            format!("Error creating index {index_name}"),
                            e,
                        )
                    })?;
                }
                None => {
                    index_created = true;
                    info!("Creating index {index_name}");
                    tx.execute(sql).map_err(|e| {
                        MigrationError::QueryFailure(
                            format!("Error creating index {index_name}"),
                            e,
                        )
                    })?;
                }
                _ => {}
            }
        }
        if !index_created {
            info!("No indexes to create");
        }
        if !index_updated {
            info!("No indexes to update");
        }

        Ok(())
    }

    pub fn parse_metadata(&mut self) -> Result<MigrationMetadata, QueryError> {
        Ok(MigrationMetadata {
            source: self.pristine.parse_metadata()?,
            target: self
                .target_connection
                .lock()
                .expect("Failed to lock mutex")
                .parse_metadata()?,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct MigrationMetadata {
    pub source: Metadata,
    pub target: Metadata,
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
