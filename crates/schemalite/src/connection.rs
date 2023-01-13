use std::{collections::HashMap, fmt::Display};

use rusqlite::{types::FromSql, Connection, Params, Row, Transaction, TransactionBehavior};
use tracing::{debug, span, trace, warn, Level};

use crate::{InitializationError, MigrationError, QueryError, SqlPrinter};

macro_rules! event {
    ($level:expr, $($args:tt)*) => {{
        match $level {
            Level::ERROR => ::tracing::event!(Level::ERROR, $($args)*),
            Level::WARN => ::tracing::event!(Level::WARN, $($args)*),
            Level::INFO => ::tracing::event!(Level::INFO, $($args)*),
            Level::DEBUG => ::tracing::event!(Level::DEBUG, $($args)*),
            Level::TRACE => ::tracing::event!(Level::TRACE, $($args)*),
        }
    }};
}

pub(crate) struct PristineConnection {
    connection: Connection,
    sql_printer: SqlPrinter,
}

impl PristineConnection {
    pub fn new() -> Result<Self, InitializationError> {
        Ok(Self {
            connection: Connection::open_in_memory()
                .map_err(|e| InitializationError::ConnectionFailure(":memory:".to_owned(), e))?,
            sql_printer: SqlPrinter::default(),
        })
    }

    pub fn initialize_schema(
        &mut self,
        schema: &[impl AsRef<str>],
    ) -> Result<(), InitializationError> {
        let init_span = span!(Level::TRACE, "Initializing schema in reference database");
        let _guard = init_span.entered();
        for definition in schema {
            trace!("\n\t{}", self.sql_printer.print(definition.as_ref()));
            self.connection
                .execute_batch(definition.as_ref())
                .map_err(|e| {
                    InitializationError::QueryFailure(
                        "Error creating schema".to_owned(),
                        QueryError(definition.as_ref().to_owned(), e),
                    )
                })?;
        }
        Ok(())
    }

    pub fn get_pragma<T: FromSql>(&mut self, pragma: &str) -> Result<T, QueryError> {
        get_pragma(
            &self.connection,
            pragma,
            Level::TRACE,
            "Executing query against reference database",
            &mut self.sql_printer,
        )
    }

    pub fn select_metadata(&mut self, sql: &str) -> Result<HashMap<String, String>, QueryError> {
        select_metadata(
            &self.connection,
            sql,
            Level::TRACE,
            "Executing query against reference database",
            &mut self.sql_printer,
        )
    }

    pub fn get_cols(&mut self, table: &str) -> Result<Vec<String>, QueryError> {
        get_cols(
            &self.connection,
            table,
            Level::TRACE,
            "Executing query against reference database",
            &mut self.sql_printer,
        )
    }
}

pub(crate) struct TargetTransaction<'conn> {
    transaction: Transaction<'conn>,
    sql_printer: SqlPrinter,
    modified: bool,
}

impl<'conn> TargetTransaction<'conn> {
    pub fn new(target_connection: &'conn mut TargetConnection) -> Result<Self, MigrationError> {
        let transaction = target_connection
            .connection
            .transaction_with_behavior(TransactionBehavior::Exclusive)
            .map_err(MigrationError::TransactionInitializationFailure)?;
        Ok(Self {
            transaction,
            sql_printer: SqlPrinter::default(),
            modified: false,
        })
    }

    pub fn execute(&mut self, sql: &str) -> Result<(), QueryError> {
        debug!("\n\t{}", self.sql_printer.print(sql));

        let rows = self
            .transaction
            .execute(sql, [])
            .map_err(|e| QueryError(sql.to_owned(), e))?;

        let normalized = sql.trim().to_uppercase();
        if normalized.starts_with("DROP")
            || sql.starts_with("ALTER")
            || sql.starts_with("INSERT")
            || sql.starts_with("CREATE")
        {
            self.modified = true;
        }
        if rows > 0 {
            debug!("Query affected {rows} row(s)");
        }
        Ok(())
    }

    pub fn select_metadata(&mut self, sql: &str) -> Result<HashMap<String, String>, QueryError> {
        select_metadata(
            &self.transaction,
            sql,
            Level::DEBUG,
            "",
            &mut self.sql_printer,
        )
    }

    pub fn query<T, F>(&mut self, sql: &str, f: F) -> Result<Vec<T>, QueryError>
    where
        F: FnMut(&Row<'_>) -> Result<T, rusqlite::Error>,
    {
        query(
            &self.transaction,
            sql,
            Level::DEBUG,
            "",
            &mut self.sql_printer,
            f,
        )
    }

    pub fn get_cols(&mut self, table: &str) -> Result<Vec<String>, QueryError> {
        get_cols(
            &self.transaction,
            table,
            Level::DEBUG,
            "",
            &mut self.sql_printer,
        )
    }

    pub fn commit(self) -> Result<(), MigrationError> {
        debug!("Committing transaction");
        self.transaction
            .commit()
            .map_err(MigrationError::TransactionCommitFailure)
    }

    pub fn rollback(self) -> Result<(), MigrationError> {
        warn!("Error during migration, rolling back");
        self.transaction
            .rollback()
            .map_err(MigrationError::TransactionRollbackFailure)
    }
}

pub(crate) struct TargetConnection {
    connection: Connection,
    sql_printer: SqlPrinter,
}

impl TargetConnection {
    pub fn new(connection: Connection) -> Self {
        Self {
            connection,
            sql_printer: SqlPrinter::default(),
        }
    }

    pub fn execute(&mut self, sql: &str) -> Result<(), QueryError> {
        debug!("\n\t{}", self.sql_printer.print(sql));

        let rows = self
            .connection
            .execute(sql, [])
            .map_err(|e| QueryError(sql.to_owned(), e))?;

        if rows > 0 {
            debug!("Query affected {rows} row(s)");
        }
        Ok(())
    }

    pub fn vacuum(&mut self) -> Result<(), QueryError> {
        debug!("Optimizing database");
        self.execute("VACUUM")
    }

    pub fn get_pragma<T: FromSql>(&mut self, pragma: &str) -> Result<T, QueryError> {
        get_pragma(
            &self.connection,
            pragma,
            Level::DEBUG,
            "",
            &mut self.sql_printer,
        )
    }
}

fn replace_sql_params<P>(sql: &str, params: P) -> String
where
    P: Params + Clone + IntoIterator + Default,
    P::Item: Display,
{
    let mut formatted_sql = sql.to_owned();
    for (i, param) in params.into_iter().enumerate() {
        formatted_sql = formatted_sql.replace(&format!("?{}", i + 1), &format!("{param}"));
    }
    formatted_sql
}

fn query_params<T, P, F>(
    connection: &Connection,
    sql: &str,
    params: P,
    log_level: Level,
    msg: &str,
    sql_printer: &mut SqlPrinter,
    f: F,
) -> Result<Vec<T>, QueryError>
where
    P: Params + Clone + IntoIterator + Default,
    P::Item: Display,
    F: FnMut(&Row<'_>) -> Result<T, rusqlite::Error>,
{
    event!(
        log_level,
        "{}\n\t{}",
        msg,
        sql_printer.print(&replace_sql_params(sql, params.clone()))
    );

    let mut statement = connection
        .prepare_cached(sql)
        .map_err(|e| QueryError(sql.to_owned(), e))?;
    let results: Result<Vec<T>, rusqlite::Error> = statement
        .query_map(params, f)
        .map_err(|e| QueryError(sql.to_owned(), e))?
        .collect();
    results.map_err(|e| QueryError(sql.to_owned(), e))
}

fn get_pragma<T: FromSql>(
    connection: &Connection,
    pragma: &str,
    log_level: Level,
    msg: &str,
    sql_printer: &mut SqlPrinter,
) -> Result<T, QueryError> {
    query_single(
        connection,
        &format!("PRAGMA {pragma}"),
        log_level,
        msg,
        sql_printer,
        |row| row.get(0),
    )
}

fn query<T, F>(
    connection: &Connection,
    sql: &str,
    log_level: Level,
    msg: &str,
    sql_printer: &mut SqlPrinter,
    f: F,
) -> Result<Vec<T>, QueryError>
where
    F: FnMut(&Row<'_>) -> Result<T, rusqlite::Error>,
{
    event!(log_level, "{}\n\t{}", msg, sql_printer.print(sql));

    let mut statement = connection
        .prepare_cached(sql)
        .map_err(|e| QueryError(sql.to_owned(), e))?;
    let results: Result<Vec<T>, rusqlite::Error> = statement
        .query_map([], f)
        .map_err(|e| QueryError(sql.to_owned(), e))?
        .collect();
    results.map_err(|e| QueryError(sql.to_owned(), e))
}

fn query_single<T, F>(
    connection: &Connection,
    sql: &str,
    log_level: Level,
    msg: &str,
    sql_printer: &mut SqlPrinter,
    f: F,
) -> Result<T, QueryError>
where
    F: FnMut(&Row<'_>) -> Result<T, rusqlite::Error>,
{
    let results = query(connection, sql, log_level, msg, sql_printer, f)?;
    Ok(results
        .into_iter()
        .next()
        .expect("Query should contain one value"))
}

fn select_metadata(
    connection: &Connection,
    sql: &str,
    log_level: Level,
    msg: &str,
    sql_printer: &mut SqlPrinter,
) -> Result<HashMap<String, String>, QueryError> {
    let results =
        query::<(String, String), _>(connection, sql, log_level, msg, sql_printer, |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;
    Ok(HashMap::from_iter(results))
}

fn get_cols(
    connection: &Connection,
    table: &str,
    log_level: Level,
    msg: &str,
    sql_printer: &mut SqlPrinter,
) -> Result<Vec<String>, QueryError> {
    query_params(
        connection,
        "SELECT name FROM pragma_table_info(?1)",
        [table],
        log_level,
        msg,
        sql_printer,
        |row| row.get(0),
    )
}
