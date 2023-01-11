use once_cell::sync::Lazy;
use regex::Regex;
use rusqlite::{types::FromSql, Connection, Params, Row, TransactionBehavior};
use std::{collections::HashMap, path::Path};
use tracing::debug;

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
    #[error("Aborting migration because data loss would occur and allow_deletions is false: {0}")]
    DataLoss(String),
    #[error("The following foreign keys have constraint violations: {0:?}")]
    ForeignKeyViolation(Vec<String>),
}

#[derive(thiserror::Error, Debug)]
#[error("Failed to execute query {0}: {1}")]
pub struct QueryError(String, #[source] rusqlite::Error);

pub struct Migrator {
    connection: Connection,
    pristine: Connection,
    options: Options,
    foreign_keys_enabled: bool,
}

#[derive(Debug, Default)]
pub struct Options {
    pub allow_deletions: bool,
}

impl Migrator {
    pub fn new(
        db_path: impl AsRef<Path>,
        schema: &[impl AsRef<str>],
        options: Options,
    ) -> Result<Self, InitializationError> {
        let connection = Connection::open(&db_path).map_err(|e| {
            InitializationError::ConnectionFailure(
                db_path.as_ref().to_string_lossy().to_string(),
                e,
            )
        })?;
        Self::init(connection, schema, options)
    }

    fn init(
        connection: Connection,
        schema: &[impl AsRef<str>],
        options: Options,
    ) -> Result<Self, InitializationError> {
        let foreign_keys_enabled = get_pragma::<i32>(&connection, "foreign_keys").map_err(|e| {
            InitializationError::QueryFailure(
                "Failed to retrieve foreign_keys pragma".to_owned(),
                e,
            )
        })? == 1;
        if foreign_keys_enabled {
            execute(&connection, "PRAGMA foreign_keys = OFF", []).map_err(|e| {
                InitializationError::QueryFailure("Failed to disable foreign keys".to_owned(), e)
            })?
        }
        let pristine = Connection::open_in_memory()
            .map_err(|e| InitializationError::ConnectionFailure(":memory:".to_owned(), e))?;
        for definition in schema {
            execute_batch(&pristine, definition.as_ref()).map_err(|e| {
                InitializationError::QueryFailure("Error creating schema".to_owned(), e)
            })?;
        }
        Ok(Self {
            connection,
            foreign_keys_enabled,
            pristine,
            options,
        })
    }

    pub fn migrate(&mut self) -> Result<(), MigrationError> {
        match self.migrate_inner() {
            Ok(changed) => {
                let pragma = "foreign_keys";
                migrate_pragma(
                    &self.connection,
                    pragma,
                    &get_pragma::<i32>(&self.pristine, pragma)
                        .map_err(|e| {
                            MigrationError::QueryFailure(
                                "Failed to retrieve foreign keys pragma".to_owned(),
                                e,
                            )
                        })?
                        .to_string(),
                    &get_pragma::<i32>(&self.connection, pragma)
                        .map_err(|e| {
                            MigrationError::QueryFailure(
                                "Failed to retrieve foreign keys pragma".to_owned(),
                                e,
                            )
                        })?
                        .to_string(),
                )
                .map_err(|e| {
                    MigrationError::QueryFailure("Error updating foreign keys pragma".to_owned(), e)
                })?;
                if changed {
                    execute(&self.connection, "VACUUM", []).map_err(|e| {
                        MigrationError::QueryFailure("Failed to vacuum database".to_owned(), e)
                    })?;
                }
            }
            Err(e) => {
                if self.foreign_keys_enabled {
                    execute(&self.connection, "PRAGMA foreign_keys = ON", []).map_err(|e| {
                        MigrationError::QueryFailure(
                            "Failed to re-enable foreign keys".to_owned(),
                            e,
                        )
                    })?;
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    fn migrate_inner(&mut self) -> Result<bool, MigrationError> {
        let mut changed = false;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Exclusive)
            .map_err(MigrationError::TransactionInitializationFailure)?;
        let pristine_tables =
            select_metadata(
                &self.pristine,
                "SELECT name, sql from sqlite_master WHERE type = 'table' and name != 'sqlite_sequence'",
            ).map_err(
                |e| MigrationError::QueryFailure("Failed to get tables from pristine database".to_owned(), e),
            )?;
        let tables =
            select_metadata(
                &tx,
                "SELECT name, sql from sqlite_master WHERE type = 'table' and name != 'sqlite_sequence'",
            ).map_err(
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
                "The follwoing tables would be removed: {removed_table_list}"
            )));
        }
        let empty = "".to_owned();
        let modified_tables = pristine_tables.iter().filter(|(name, sql)| {
            normalize_sql(tables.get(*name).unwrap_or(&empty)) != normalize_sql(sql)
        });
        execute(&tx, "PRAGMA defer_foreign_keys = TRUE", []).map_err(|e| {
            MigrationError::QueryFailure("Error enabling defer_foreign_keys".to_owned(), e)
        })?;
        for (new_table, new_table_sql) in new_tables {
            changed = true;
            execute(&tx, new_table_sql, []).map_err(|e| {
                MigrationError::QueryFailure(format!("Error creating table {new_table}"), e)
            })?;
        }
        for removed_table in removed_tables {
            changed = true;
            execute(&tx, &format!("DROP TABLE {removed_table}"), []).map_err(|e| {
                MigrationError::QueryFailure(format!("Error dropping table {removed_table}"), e)
            })?;
        }
        for (modified_table, modified_table_sql) in modified_tables {
            changed = true;
            let temp_table = format!("{modified_table}_migration_new");
            let create_table_regex = Regex::new(&format!(r"\b{}\b", regex::escape(modified_table)))
                .expect("Regex should compile");

            let create_temp_table_sql =
                create_table_regex.replace_all(modified_table_sql, &temp_table);
            execute(&tx, &create_temp_table_sql, []).map_err(|e| {
                MigrationError::QueryFailure(format!("Error creating temp table {temp_table}"), e)
            })?;

            let cols = get_cols(&tx, modified_table).map_err(|e| {
                MigrationError::QueryFailure(
                    format!("Error getting columns for table {modified_table}"),
                    e,
                )
            })?;

            let pristine_cols = get_cols(&self.pristine, modified_table).map_err(|e| {
                MigrationError::QueryFailure(
                    format!("Error getting columns for table {modified_table}"),
                    e,
                )
            })?;

            let has_removed_cols = cols.iter().any(|c| !pristine_cols.contains(c));
            if !self.options.allow_deletions && has_removed_cols {
                panic!("fix");
            }
            let common_cols = cols
                .into_iter()
                .filter(|c| pristine_cols.contains(c))
                .collect::<Vec<_>>()
                .join(",");
            execute(
                &tx,
                &format!(
                    r#"INSERT INTO {temp_table} ({common_cols})
                    SELECT {common_cols} FROM {modified_table}"#
                ),
                [],
            )
            .map_err(|e| {
                MigrationError::QueryFailure(
                    format!("Error migrating data into table {modified_table}"),
                    e,
                )
            })?;

            execute(&tx, &format!("DROP TABLE {modified_table}"), []).map_err(|e| {
                MigrationError::QueryFailure(format!("Error dropping table {modified_table}"), e)
            })?;

            execute(
                &tx,
                &format!("ALTER TABLE {temp_table} RENAME TO {modified_table}"),
                [],
            )
            .map_err(|e| {
                MigrationError::QueryFailure(
                    format!("Error renaming {temp_table} to {modified_table}"),
                    e,
                )
            })?;
        }
        let pristine_indexes = select_metadata(
            &self.pristine,
            "SELECT name, sql FROM sqlite_master WHERE type = 'index'",
        )
        .map_err(|e| {
            MigrationError::QueryFailure(
                "Failed to get indexes from pristine database".to_owned(),
                e,
            )
        })?;

        let indexes = select_metadata(
            &tx,
            "SELECT name, sql FROM sqlite_master WHERE type = 'index'",
        )
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
            changed = true;
            execute(&tx, &format!("DROP INDEX {index}"), []).map_err(|e| {
                MigrationError::QueryFailure(format!("Failed to drop index {index}"), e)
            })?;
        }

        for (index_name, sql) in pristine_indexes {
            match indexes.get(&index_name) {
                Some(old_index) if normalize_sql(&sql) != normalize_sql(old_index) => {
                    changed = true;

                    execute(&tx, &format!("DROP INDEX {index_name}"), []).map_err(|e| {
                        MigrationError::QueryFailure(
                            format!("Error dropping index {index_name}"),
                            e,
                        )
                    })?;
                    execute(&tx, &sql, []).map_err(|e| {
                        MigrationError::QueryFailure(
                            format!("Error creating index {index_name}"),
                            e,
                        )
                    })?;
                }
                None => {
                    changed = true;
                    execute(&tx, &sql, []).map_err(|e| {
                        MigrationError::QueryFailure(
                            format!("Error creating index {index_name}"),
                            e,
                        )
                    })?;
                }
                _ => {
                    debug!("Index {index_name} is unchanged, skipping");
                }
            }
        }
        let pragma = "user_version";
        migrate_pragma(
            &tx,
            pragma,
            &get_pragma::<i32>(&self.pristine, pragma)
                .map_err(|e| {
                    MigrationError::QueryFailure(
                        "Failed to get user_version pragma from pristine database".to_owned(),
                        e,
                    )
                })?
                .to_string(),
            &get_pragma::<i32>(&tx, pragma)
                .map_err(|e| {
                    MigrationError::QueryFailure(
                        "Failed to get user_version pragma from current database".to_owned(),
                        e,
                    )
                })?
                .to_string(),
        )
        .map_err(|e| {
            MigrationError::QueryFailure("Error updating user_version pragma".to_owned(), e)
        })?;
        if get_pragma::<i32>(&self.pristine, "foreign_keys").map_err(|e| {
            MigrationError::QueryFailure(
                "Failed to get foreign_keys pragma from pristine database".to_owned(),
                e,
            )
        })? == 1
        {
            let foreign_key_violations: Vec<String> =
                query(&tx, "PRAGMA foreign_key_check", [], |row| row.get(0)).map_err(|e| {
                    MigrationError::QueryFailure("Error executing foreign key check".to_owned(), e)
                })?;

            if !foreign_key_violations.is_empty() {
                return Err(MigrationError::ForeignKeyViolation(foreign_key_violations));
            }
        }
        tx.commit()
            .map_err(MigrationError::TransactionCommitFailure)?;
        Ok(changed)
    }
}

fn query<T, F>(
    connection: &Connection,
    sql: &str,
    params: impl Params,
    f: F,
) -> Result<Vec<T>, QueryError>
where
    F: FnMut(&Row<'_>) -> Result<T, rusqlite::Error>,
{
    debug!("Executing query: {sql}");
    let mut statement = connection
        .prepare_cached(sql)
        .map_err(|e| QueryError(sql.to_owned(), e))?;
    let results: Result<Vec<T>, rusqlite::Error> = statement
        .query_map(params, f)
        .map_err(|e| QueryError(sql.to_owned(), e))?
        .collect();
    results.map_err(|e| QueryError(sql.to_owned(), e))
}

fn query_single<T, F>(
    connection: &Connection,
    sql: &str,
    params: impl Params,
    f: F,
) -> Result<T, QueryError>
where
    F: FnMut(&Row<'_>) -> Result<T, rusqlite::Error>,
{
    let results = query(connection, sql, params, f)?;
    Ok(results
        .into_iter()
        .next()
        .expect("Query should contain one value"))
}

fn execute(connection: &Connection, sql: &str, params: impl Params) -> Result<(), QueryError> {
    debug!("Executing query: {sql}");
    let rows = connection
        .execute(sql, params)
        .map_err(|e| QueryError(sql.to_owned(), e))?;
    debug!("Query affected {rows} row(s)");
    Ok(())
}

fn execute_batch(connection: &Connection, sql: &str) -> Result<(), QueryError> {
    debug!("Executing query: {sql}");
    connection
        .execute_batch(sql)
        .map_err(|e| QueryError(sql.to_owned(), e))
}

fn get_pragma<T: FromSql>(connection: &Connection, pragma: &str) -> Result<T, QueryError> {
    query_single(connection, &format!("PRAGMA {pragma}"), [], |row| {
        row.get(0)
    })
}

fn migrate_pragma(
    connection: &Connection,
    pragma: &str,
    pristine_val: &str,
    current_val: &str,
) -> Result<(), QueryError> {
    if current_val != pristine_val {
        execute(connection, &format!("PRAGMA {pragma} = {pristine_val}"), [])
    } else {
        Ok(())
    }
}

fn select_metadata(
    connection: &Connection,
    sql: &str,
) -> Result<HashMap<String, String>, QueryError> {
    let results =
        query::<(String, String), _>(connection, sql, [], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(HashMap::from_iter(results))
}

fn get_cols(connection: &Connection, table: &str) -> Result<Vec<String>, QueryError> {
    query(
        connection,
        "SELECT name FROM pragma_table_info(?)",
        [table],
        |row| row.get(0),
    )
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
