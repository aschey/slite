use crate::{connection::Metadata, error::QueryError, unified_diff_builder::UnifiedDiffBuilder};
use crate::{Migrator, SqlPrinter};
use imara_diff::{diff, intern::InternedInput, Algorithm};
use std::collections::HashMap;

impl Migrator {
    pub fn diff(&mut self) -> Result<String, QueryError> {
        let metadata = self.parse_metadata()?;

        let source_str = build_full_schema_string(&metadata.source);
        let target_str = build_full_schema_string(&metadata.target);

        Ok(sql_diff(source_str.as_str(), target_str.as_str()))
    }
}

pub fn sql_diff(source: &str, target: &str) -> String {
    let input = InternedInput::new(source, target);
    let diff_result = diff(
        Algorithm::Histogram,
        &input,
        UnifiedDiffBuilder::new(&input),
    );
    if diff_result.is_empty() {
        return format!("\n{}", SqlPrinter::default().print(target));
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

fn build_schema_string(metadata: &HashMap<String, String>) -> String {
    let mut names: Vec<&String> = metadata.keys().collect();
    names.sort();

    names
        .into_iter()
        .map(|n| metadata.get(n).unwrap().to_owned())
        .collect::<Vec<_>>()
        .join("\n")
}
