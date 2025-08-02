use std::path::PathBuf;

use rusqlite::{Connection, OpenFlags};

use crate::error::InitializationError;
use crate::{Config, MigrationMetadata, Migrator, Options, read_sql_files};

#[derive(Debug, Clone)]
pub struct MigratorFactory {
    schema_dir: PathBuf,
    schemas: Vec<String>,
    target_db_path: PathBuf,
    metadata: MigrationMetadata,
    open_flags: OpenFlags,
    config: Config,
}

impl MigratorFactory {
    pub fn new(
        schema_dir: impl Into<PathBuf>,
        target_db_path: impl Into<PathBuf>,
        config: Config,
    ) -> Result<Self, InitializationError> {
        let mut factory = Self {
            schemas: vec![],
            schema_dir: schema_dir.into(),
            target_db_path: target_db_path.into(),
            open_flags: OpenFlags::default(),
            metadata: MigrationMetadata::default(),
            config,
        };
        factory.update_schemas()?;
        Ok(factory)
    }

    pub fn with_open_flags(self, open_flags: OpenFlags) -> Self {
        Self { open_flags, ..self }
    }

    pub fn set_config(&mut self, config: Config) {
        self.config = config;
    }

    pub fn set_schema_dir(&mut self, dir: PathBuf) {
        self.schema_dir = dir;
    }

    pub fn set_target_path(&mut self, path: PathBuf) {
        self.target_db_path = path;
    }

    pub fn create_migrator(&self, options: Options) -> Result<Migrator, InitializationError> {
        Migrator::new(
            &self.schemas,
            Connection::open_with_flags(&self.target_db_path, self.open_flags).unwrap(),
            self.config.clone(),
            options,
        )
    }

    pub fn schema_dir(&self) -> &PathBuf {
        &self.schema_dir
    }

    pub fn metadata(&self) -> &MigrationMetadata {
        &self.metadata
    }

    pub fn update_schemas(&mut self) -> Result<(), InitializationError> {
        self.schemas = read_sql_files(&self.schema_dir);

        self.metadata = self
            .create_migrator(Options {
                allow_deletions: false,
                dry_run: true,
            })?
            .parse_metadata()
            .map_err(|e| {
                InitializationError::QueryFailure("Failed to parse metadata".to_owned(), e)
            })?;
        Ok(())
    }
}
