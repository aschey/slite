use crate::{error::QueryError, unified_diff_builder::UnifiedDiffBuilder};
use crate::{MigrationMetadata, Migrator, SqlPrinter};
use imara_diff::{diff, intern::InternedInput, Algorithm};

impl Migrator {
    pub fn diff(&mut self) -> Result<String, QueryError> {
        let metadata = self.parse_metadata()?;

        let diffs = diff_objects(metadata);
        Ok(diffs
            .into_iter()
            .filter_map(|d| {
                if d.diff_text.is_empty() {
                    None
                } else {
                    Some(d.diff_text)
                }
            })
            .collect::<Vec<_>>()
            .join("\n"))
    }
}

pub struct Diff {
    pub diff_text: String,
    pub original_text: String,
}

pub fn diff_objects(metadata: MigrationMetadata) -> Vec<Diff> {
    let objects = metadata.clone().into_objects();
    objects
        .tables
        .iter()
        .map(|t| {
            sql_diff(
                &metadata.source.tables.get(t).cloned().unwrap_or_default(),
                &metadata.target.tables.get(t).cloned().unwrap_or_default(),
            )
        })
        .chain(objects.indexes.iter().map(|t| {
            sql_diff(
                &metadata.source.indexes.get(t).cloned().unwrap_or_default(),
                &metadata.target.indexes.get(t).cloned().unwrap_or_default(),
            )
        }))
        .chain(objects.views.iter().map(|t| {
            sql_diff(
                &metadata.source.views.get(t).cloned().unwrap_or_default(),
                &metadata.target.views.get(t).cloned().unwrap_or_default(),
            )
        }))
        .chain(objects.triggers.iter().map(|t| {
            sql_diff(
                &metadata.source.triggers.get(t).cloned().unwrap_or_default(),
                &metadata.target.triggers.get(t).cloned().unwrap_or_default(),
            )
        }))
        .collect()
}

pub fn sql_diff(source: &str, target: &str) -> Diff {
    let input = InternedInput::new(target, source);
    Diff {
        diff_text: diff(
            Algorithm::Histogram,
            &input,
            UnifiedDiffBuilder::new(&input),
        ),
        original_text: SqlPrinter::default().print(source),
    }
}
