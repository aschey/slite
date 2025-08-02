use std::collections::BTreeMap;
use std::ops::Deref;

use imara_diff::intern::InternedInput;
use imara_diff::{Algorithm, diff};

use crate::error::QueryError;
use crate::unified_diff_builder::UnifiedDiffBuilder;
use crate::{MigrationMetadata, Migrator, ObjectType, SqlPrinter};

impl Migrator {
    pub fn diff(&mut self) -> Result<String, QueryError> {
        let metadata = self.parse_metadata()?;

        let diffs = diff_metadata(metadata);
        Ok(diffs
            .0
            .values()
            .flat_map(|d| d.values())
            .filter_map(|d| {
                if d.diff_text.is_empty() {
                    None
                } else {
                    Some(d.diff_text.clone())
                }
            })
            .collect::<Vec<_>>()
            .join("\n"))
    }
}

pub struct SchemaDiff(BTreeMap<ObjectType, BTreeMap<String, Diff>>);

impl Deref for SchemaDiff {
    type Target = BTreeMap<ObjectType, BTreeMap<String, Diff>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct Diff {
    pub diff_text: String,
    pub original_text: String,
    pub new_text: String,
}

pub fn diff_metadata(metadata: MigrationMetadata) -> SchemaDiff {
    let mut map = BTreeMap::<ObjectType, BTreeMap<String, Diff>>::default();
    map.insert(ObjectType::Table, Default::default());
    map.insert(ObjectType::Index, Default::default());
    map.insert(ObjectType::View, Default::default());
    map.insert(ObjectType::Trigger, Default::default());
    let diffs = metadata
        .unified_objects()
        .iter()
        .map(|o| {
            (
                o,
                diff_objects(
                    &o.name,
                    metadata.source.get(&o.object_type),
                    metadata.target.get(&o.object_type),
                ),
            )
        })
        .fold(map, |mut acc, (object, diff)| {
            acc.get_mut(&object.object_type)
                .unwrap()
                .insert(object.name.clone(), diff);
            acc
        });
    SchemaDiff(diffs)
}

fn diff_objects(
    name: &str,
    source: &BTreeMap<String, String>,
    target: &BTreeMap<String, String>,
) -> Diff {
    sql_diff(
        source.get(name).map(|s| s.as_str()).unwrap_or_default(),
        target.get(name).map(|s| s.as_str()).unwrap_or_default(),
    )
}

pub fn sql_diff(source: &str, target: &str) -> Diff {
    let input = InternedInput::new(target, source);
    Diff {
        diff_text: diff(
            Algorithm::Histogram,
            &input,
            UnifiedDiffBuilder::new(&input),
        ),
        original_text: if source.is_empty() {
            String::default()
        } else {
            SqlPrinter::default().print(source)
        },
        new_text: if target.is_empty() {
            String::default()
        } else {
            SqlPrinter::default().print(target)
        },
    }
}
