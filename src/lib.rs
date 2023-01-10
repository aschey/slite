use once_cell::sync::Lazy;
use regex::Regex;
use rusqlite::{types::FromSql, Connection, TransactionBehavior};
use std::{collections::HashMap, error::Error, path::Path};

macro_rules! regex {
    ($name: ident, $re:literal $(,)?) => {
        static $name: Lazy<Regex> = Lazy::new(|| Regex::new($re).unwrap());
    };
}

regex!(COMMENTS_RE, r"--[^\n]*\n");
regex!(WHITESPACE_RE, r"\s+");
regex!(EXTRA_WHITESPACE_RE, r" *([(),]) *");
regex!(QUOTES_RE, r#""(\w+)""#);

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
    pub fn new(db_path: impl AsRef<Path>, schema: &[impl AsRef<str>], options: Options) -> Self {
        let connection = Connection::open(db_path).unwrap();
        Self::init(connection, schema, options)
    }

    fn init(connection: Connection, schema: &[impl AsRef<str>], options: Options) -> Self {
        let foreign_keys_enabled = get_pragma::<i32>(&connection, "foreign_keys") == 1;
        if foreign_keys_enabled {
            connection.execute("PRAGMA foreign_keys = OFF", []).unwrap();
        }
        let pristine = Connection::open_in_memory().unwrap();

        for definition in schema {
            pristine.execute_batch(definition.as_ref()).unwrap();
        }

        Self {
            connection,
            foreign_keys_enabled,
            pristine,
            options,
        }
    }

    pub fn migrate(&mut self) -> Result<(), Box<dyn Error>> {
        match self.migrate_inner() {
            Ok(changed) => {
                let pragma = "foreign_keys";
                migrate_pragma(
                    &self.connection,
                    pragma,
                    &get_pragma::<i32>(&self.pristine, pragma).to_string(),
                    &get_pragma::<i32>(&self.connection, pragma).to_string(),
                );
                if changed {
                    self.connection.execute("VACUUM", []).unwrap();
                }
            }
            Err(e) => {
                if self.foreign_keys_enabled {
                    self.connection
                        .execute("PRAGMA foreign_keys = ON", [])
                        .unwrap();
                }
            }
        }

        Ok(())
    }

    fn migrate_inner(&mut self) -> Result<bool, Box<dyn Error>> {
        let mut changed = false;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Exclusive)
            .unwrap();
        let pristine_tables = select_metadata(&self.pristine, "SELECT name, sql from sqlite_master WHERE type = 'table' and name != 'sqlite_sequence'");

        let tables = select_metadata(& tx,"SELECT name, sql from sqlite_master WHERE type = 'table' and name != 'sqlite_sequence'");

        let new_tables: Vec<&String> = pristine_tables
            .keys()
            .filter(|k| !tables.contains_key(*k))
            .collect();
        let removed_tables: Vec<&String> = tables
            .keys()
            .filter(|k| !pristine_tables.contains_key(*k))
            .collect();

        if !removed_tables.is_empty() && !self.options.allow_deletions {
            panic!("fix")
        }

        let empty = "".to_owned();
        let modified_tables = pristine_tables.iter().filter(|(name, sql)| {
            normalize_sql(tables.get(*name).unwrap_or(&empty)) != normalize_sql(sql)
        });
        tx.execute("PRAGMA defer_foreign_keys = TRUE", []).unwrap();

        for new_table in new_tables {
            changed = true;
            tx.execute(pristine_tables.get(new_table).unwrap(), [])
                .unwrap();
        }

        for removed_table in removed_tables {
            changed = true;
            tx.execute(&format!("DROP TABLE {removed_table}"), [])
                .unwrap();
        }

        for (modified_table, _) in modified_tables {
            changed = true;
            let create_table_sql = pristine_tables.get(modified_table).unwrap();
            let create_table_regex =
                Regex::new(&format!(r"\b{}\b", regex::escape(modified_table))).unwrap();
            let create_table_sql = create_table_regex
                .replace_all(create_table_sql, format!("{modified_table}_migration_new"));
            tx.execute(&create_table_sql, []).unwrap();

            let cols = get_cols(&tx, modified_table);
            let pristine_cols = get_cols(&self.pristine, modified_table);
            let has_removed_cols = cols.iter().any(|c| !pristine_cols.contains(c));
            if !self.options.allow_deletions && has_removed_cols {
                panic!("fix");
            }
            let common_cols = cols
                .into_iter()
                .filter(|c| pristine_cols.contains(c))
                .collect::<Vec<_>>()
                .join(",");

            tx.execute(
                &format!(
                    r#"INSERT INTO {modified_table}_migration_new ({common_cols})
                        SELECT {common_cols} FROM {modified_table}"#
                ),
                [],
            )
            .unwrap();

            tx.execute(&format!("DROP TABLE {modified_table}"), [])
                .unwrap();
            tx.execute(
                &format!("ALTER TABLE {modified_table}_migration_new RENAME TO {modified_table}"),
                [],
            )
            .unwrap();
        }
        let pristine_indexes = select_metadata(
            &self.pristine,
            "SELECT name, sql FROM sqlite_master WHERE type = 'index'",
        );
        let indexes = select_metadata(
            &tx,
            "SELECT name, sql FROM sqlite_master WHERE type = 'index'",
        );
        let old_indexes = indexes
            .keys()
            .filter(|k| !pristine_indexes.contains_key(*k));
        for index in old_indexes {
            changed = true;
            tx.execute(&format!("DROP INDEX {index}"), []).unwrap();
        }
        for (index_name, sql) in pristine_indexes {
            if !indexes.contains_key(&index_name) {
                changed = true;
                tx.execute(&sql, []).unwrap();
            } else if sql != *indexes.get(&index_name).unwrap() {
                changed = true;
                tx.execute(&format!("DROP INDEX {index_name}"), []).unwrap();
                tx.execute(&sql, []).unwrap();
            }
        }

        let pragma = "user_version";
        migrate_pragma(
            &tx,
            pragma,
            &get_pragma::<i32>(&self.pristine, pragma).to_string(),
            &get_pragma::<i32>(&tx, pragma).to_string(),
        );

        if get_pragma::<i32>(&self.pristine, "foreign_keys") == 1 {
            let mut foreign_key_check = tx.prepare("PRAGMA foreign_key_check").unwrap();
            let foreign_key_violations: Vec<String> = foreign_key_check
                .query_map([], |row| row.get(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect();
            if !foreign_key_violations.is_empty() {
                panic!("{}", foreign_key_violations.join(" "))
            }
        }
        tx.commit().unwrap();
        Ok(changed)
    }
}

fn get_pragma<T: FromSql>(connection: &Connection, pragma: &str) -> T {
    connection
        .query_row(&format!("PRAGMA {pragma}"), [], |row| row.get(0))
        .unwrap()
}

fn migrate_pragma(connection: &Connection, pragma: &str, pristine_val: &str, current_val: &str) {
    if current_val != pristine_val {
        connection
            .execute(&format!("PRAGMA {pragma} = {pristine_val}"), [])
            .unwrap();
    }
}

fn select_metadata(connection: &Connection, sql: &str) -> HashMap<String, String> {
    let mut statement = connection.prepare(sql).unwrap();
    statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}

fn get_cols(connection: &Connection, table: &str) -> Vec<String> {
    connection
        .prepare("SELECT name FROM pragma_table_info(?)")
        .unwrap()
        .query_map([table], |row| row.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
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
