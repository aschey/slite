use crate::{connection::Metadata, error::QueryError, unified_diff_builder::UnifiedDiffBuilder};
use crate::{Migrator, SqlPrinter, OUTPUT_IS_TTY};
use imara_diff::{diff, intern::InternedInput, Algorithm};
use std::collections::BTreeMap;

impl Migrator {
    pub fn diff(&mut self) -> Result<String, QueryError> {
        let metadata = self.parse_metadata()?;

        let source_str = build_full_schema_string(&metadata.source);
        let target_str = build_full_schema_string(&metadata.target);

        Ok(sql_diff(source_str.as_str(), target_str.as_str()))
    }
}

pub fn sql_diff(source: &str, target: &str) -> String {
    let input = InternedInput::new(target, source);
    let diff_result = if *OUTPUT_IS_TTY {
        diff(
            Algorithm::Histogram,
            &input,
            UnifiedDiffBuilder::new(&input),
        )
    } else {
        diff(
            Algorithm::Histogram,
            &input,
            imara_diff::UnifiedDiffBuilder::new(&input),
        )
    };
    if diff_result.is_empty() {
        return format!("\n  {}", SqlPrinter::default().print(target));
    }
    diff_result
}

fn build_full_schema_string(metadata: &Metadata) -> String {
    format!(
        "{}\n\n{}",
        build_schema_string(&metadata.tables),
        build_schema_string(&metadata.indexes)
    )
}

fn build_schema_string(metadata: &BTreeMap<String, String>) -> String {
    metadata
        .values()
        .map(|v| v.to_owned())
        .collect::<Vec<_>>()
        .join("\n\n")
}
