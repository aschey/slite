use crate::{read_sql_files, MigrationMetadata, Migrator, Options};
use rusqlite::{Connection, OpenFlags};
use std::path::PathBuf;

pub struct MigratorFactory {
    schema_dir: PathBuf,
    schemas: Vec<String>,
    target_db_path: PathBuf,
    metadata: MigrationMetadata,
    open_flags: OpenFlags,
}

impl MigratorFactory {
    pub fn new(schema_dir: impl Into<PathBuf>, target_db_path: impl Into<PathBuf>) -> Self {
        let mut factory = Self {
            schemas: vec![],
            schema_dir: schema_dir.into(),
            target_db_path: target_db_path.into(),
            open_flags: OpenFlags::default(),
            metadata: MigrationMetadata::default(),
        };
        factory.update_schemas();
        factory
    }

    pub fn with_open_flags(self, open_flags: OpenFlags) -> Self {
        Self { open_flags, ..self }
    }

    pub fn create_migrator(&self, options: Options) -> Migrator {
        Migrator::new(
            &self.schemas,
            Connection::open_with_flags(&self.target_db_path, self.open_flags).unwrap(),
            options,
        )
        .unwrap()
    }

    pub fn schema_dir(&self) -> &PathBuf {
        &self.schema_dir
    }

    pub fn metadata(&self) -> &MigrationMetadata {
        &self.metadata
    }

    pub fn update_schemas(&mut self) {
        self.schemas = read_sql_files(&self.schema_dir);

        self.metadata = self
            .create_migrator(Options {
                allow_deletions: false,
                dry_run: true,
            })
            .parse_metadata()
            .unwrap();
    }
}
