use std::{collections::BTreeMap, ops::Deref};

use regex::Regex;
use rusqlite::Connection;
use tracing::Level;

use crate::{query, Object, ObjectType, QueryError, SqlPrinter};

#[derive(Clone, Debug, Default)]
pub struct Metadata(BTreeMap<ObjectType, BTreeMap<String, String>>);

impl Deref for Metadata {
    type Target = BTreeMap<ObjectType, BTreeMap<String, String>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Metadata {
    pub fn unified_objects(&self, other: &Metadata) -> Vec<Object> {
        let mut all: Vec<_> = self
            .all_objects()
            .iter()
            .chain(other.all_objects().iter())
            .map(|k| k.to_owned())
            .collect();
        all.dedup();
        all
    }

    pub fn all_objects(&self) -> Vec<Object> {
        self.0
            .iter()
            .flat_map(|(object_type, objects)| {
                objects.iter().map(|(name, sql)| Object {
                    name: name.to_owned(),
                    sql: sql.to_owned(),
                    object_type: object_type.to_owned(),
                })
            })
            .collect()
    }

    pub(crate) fn parse(
        connection: &Connection,
        log_level: Level,
        msg: &str,
        ignore: &Option<Regex>,
        sql_printer: &mut SqlPrinter,
    ) -> Result<Metadata, QueryError> {
        let metadata_sql = |name: &str| {
            format!("SELECT name, sql from sqlite_master WHERE type = '{name}' and name != 'sqlite_sequence' AND sql IS NOT NULL ORDER BY name")
        };

        let tables = select_metadata(
            connection,
            &metadata_sql("table"),
            log_level,
            msg,
            ignore,
            sql_printer,
        )?;

        let indexes = select_metadata(
            connection,
            &metadata_sql("index"),
            log_level,
            msg,
            ignore,
            sql_printer,
        )?;

        let triggers = select_metadata(
            connection,
            &metadata_sql("trigger"),
            log_level,
            msg,
            ignore,
            sql_printer,
        )?;

        let views = select_metadata(
            connection,
            &metadata_sql("view"),
            log_level,
            msg,
            ignore,
            sql_printer,
        )?;

        let mut map = BTreeMap::<ObjectType, BTreeMap<String, String>>::new();
        map.insert(ObjectType::Table, tables);
        map.insert(ObjectType::Index, indexes);
        map.insert(ObjectType::View, views);
        map.insert(ObjectType::Trigger, triggers);

        Ok(Metadata(map))
    }

    pub fn get(&self, object_type: &ObjectType) -> &BTreeMap<String, String> {
        self.0.get(object_type).unwrap()
    }

    pub fn tables(&self) -> &BTreeMap<String, String> {
        self.0.get(&ObjectType::Table).unwrap()
    }

    pub fn indexes(&self) -> &BTreeMap<String, String> {
        self.0.get(&ObjectType::Index).unwrap()
    }

    pub fn views(&self) -> &BTreeMap<String, String> {
        self.0.get(&ObjectType::View).unwrap()
    }

    pub fn triggers(&self) -> &BTreeMap<String, String> {
        self.0.get(&ObjectType::Trigger).unwrap()
    }
}

fn select_metadata(
    connection: &Connection,
    sql: &str,
    log_level: Level,
    msg: &str,
    ignore: &Option<Regex>,
    sql_printer: &mut SqlPrinter,
) -> Result<BTreeMap<String, String>, QueryError> {
    let results =
        query::<(String, String), _>(connection, sql, log_level, msg, sql_printer, |row| {
            Ok((row.get(0)?, row.get::<_, String>(1)?))
        })?
        .into_iter()
        .filter(|(key, _)| !ignore.as_ref().map(|i| i.is_match(key)).unwrap_or(false));
    Ok(BTreeMap::from_iter(results))
}
