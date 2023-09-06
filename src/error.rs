use std::io;
use std::path::PathBuf;

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
pub struct QueryError(pub(crate) String, #[source] pub(crate) rusqlite::Error);

#[derive(thiserror::Error, Debug)]
pub enum SqlFormatError {
    #[error("Error formatting SQL {0}: {1}")]
    TextFormattingFailure(String, #[source] ansi_to_tui::Error),
}

#[derive(thiserror::Error, Debug)]
pub enum RefreshError {
    #[error("{0}")]
    SqlFormatFailure(#[source] SqlFormatError),
    #[error("{0}")]
    InitializationFailure(#[source] InitializationError),
    #[error("{0}")]
    IoFailure(#[source] io::Error),
}

#[derive(thiserror::Error, Debug)]
#[error("Error loading config file {0}: {1}")]
pub struct ConfigLoadError(pub(crate) PathBuf, pub(crate) String);
