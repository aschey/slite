use rstest::rstest;
use rusqlite::{Connection, OpenFlags};

use crate::{normalize_sql, MigrationError, Migrator, Options};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct SqliteMetadata {
    metadata_type: String,
    name: String,
    tbl_name: String,
    sql: String,
}

#[rstest]
fn test_schema_migration(#[values(0, 1, 2, 3, 4)] from: usize, #[values(0, 1, 2, 3, 4)] to: usize) {
    let schemas = schemas();
    let need_allow_deletions = matches!(
        (from, to),
        (1, 0) | (2, 0) | (2, 1) | (2, 3) | (2, 4) | (3, 0) | (3, 1) | (4, 0) | (4, 1)
    );
    let connection = get_connection(&format!("{from}{to}"));
    let connection2 = get_connection(&format!("{from}{to}"));
    connection.execute_batch(schemas[from]).unwrap();

    if need_allow_deletions {
        let connection = get_connection(&format!("{from}{to}error"));
        let connection2 = get_connection(&format!("{from}{to}error"));
        connection.execute_batch(schemas[from]).unwrap();
        let migrator = Migrator::new(
            &[schemas[to]],
            connection,
            crate::Config::default(),
            Options::default(),
        )
        .unwrap();
        let result = migrator.migrate();
        assert!(matches!(result, Err(MigrationError::DataLoss(_))));
        assert_schema_equal(&connection2, schemas[from]);
    }
    let migrator = Migrator::new(
        &[schemas[to]],
        connection,
        crate::Config::default(),
        Options {
            allow_deletions: need_allow_deletions,
            ..Default::default()
        },
    )
    .unwrap();
    migrator.migrate().unwrap();
    assert_schema_equal(&connection2, schemas[to]);
}

#[rstest]
fn test_data_migration() {
    let schemas = schemas();
    let get_connection = || get_connection("123");
    // Ensure at least one connection stays alive so the memory db isn't dropped.
    let _connection = get_connection();
    let connection = get_connection();
    connection.execute_batch(schemas[1]).unwrap();

    let mut statement = connection
        .prepare("INSERT INTO Node(node_oid, node_id) VALUES (?, ?)")
        .unwrap();
    statement.execute([0, 0]).unwrap();
    statement.execute([1, 100]).unwrap();

    let migrator = Migrator::new(
        &[schemas[2]],
        get_connection(),
        crate::Config::default(),
        Options::default(),
    )
    .unwrap();
    migrator.migrate().unwrap();
    let connection = get_connection();

    let mut statement = connection
        .prepare("SELECT node_oid, node_id, active FROM Node")
        .unwrap();
    let results: Vec<(i32, String, i32)> = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!((0, "0".to_owned(), 1), results.first().unwrap().clone());
    assert_eq!((1, "100".to_owned(), 1), results.get(1).unwrap().clone());
    connection
        .execute(
            "UPDATE Node SET active = 0, node_id = 'abc' WHERE node_oid = 0",
            [],
        )
        .unwrap();
    let mut statement = connection
        .prepare("INSERT INTO Job(node_oid, id) VALUES (?,?)")
        .unwrap();
    statement.execute([0, 1234]).unwrap();
    statement.execute([0, 5432]).unwrap();
    statement.execute([1, 1234]).unwrap();
    statement.execute([1, 9876]).unwrap();
    let mut statement = connection
        .prepare("SELECT node_id, id FROM Job INNER JOIN Node ON Node.node_oid == Job.node_oid")
        .unwrap();
    let rows: Vec<(String, i32)> = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert_eq!(("abc".to_owned(), 1234), rows.first().unwrap().clone());
    assert_eq!(("abc".to_owned(), 5432), rows.get(1).unwrap().clone());
    assert_eq!(("100".to_owned(), 1234), rows.get(2).unwrap().clone());
    assert_eq!(("100".to_owned(), 9876), rows.get(3).unwrap().clone());

    let migrator = Migrator::new(
        &[schemas[3]],
        get_connection(),
        crate::Config::default(),
        Options {
            allow_deletions: true,
            ..Default::default()
        },
    )
    .unwrap();
    migrator.migrate().unwrap();
    let connection = get_connection();

    let mut statement = connection
        .prepare("SELECT node_id, id FROM Job INNER JOIN Node ON Node.node_oid == Job.node_oid")
        .unwrap();
    let rows: Vec<(String, i32)> = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(("abc".to_owned(), 1234), rows.first().unwrap().clone());
    assert_eq!(("abc".to_owned(), 5432), rows.get(1).unwrap().clone());
    assert_eq!(("100".to_owned(), 1234), rows.get(2).unwrap().clone());
    assert_eq!(("100".to_owned(), 9876), rows.get(3).unwrap().clone());

    let migrator = Migrator::new(
        &[schemas[4]],
        get_connection(),
        crate::Config::default(),
        Options::default(),
    )
    .unwrap();
    migrator.migrate().unwrap();

    let mut statement = connection
        .prepare("SELECT node_oid, node_id, active FROM Node")
        .unwrap();
    let rows: Vec<(i32, String, i32)> = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!((0, "abc".to_owned(), 0), rows.first().unwrap().clone());
    assert_eq!((1, "100".to_owned(), 1), rows.get(1).unwrap().clone());

    connection
        .execute(
            "UPDATE Node SET active = 0, node_id = '0' WHERE node_oid == 0",
            [],
        )
        .unwrap();
    let migrator = Migrator::new(
        &[schemas[1]],
        get_connection(),
        crate::Config::default(),
        Options {
            allow_deletions: true,
            ..Default::default()
        },
    )
    .unwrap();
    migrator.migrate().unwrap();

    let connection = get_connection();
    let mut statement = connection
        .prepare("SELECT node_oid, node_id FROM Node")
        .unwrap();
    let rows: Vec<(i32, i32)> = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!((0, 0), rows.first().unwrap().clone());
    assert_eq!((1, 100), rows.get(1).unwrap().clone());
}

fn get_connection(name: &str) -> Connection {
    Connection::open_with_flags(
        format!("file:memdb{name}"),
        OpenFlags::default() | OpenFlags::SQLITE_OPEN_MEMORY | OpenFlags::SQLITE_OPEN_SHARED_CACHE,
    )
    .unwrap()
}

fn dump_sqlite_master(connection: &Connection) -> Vec<SqliteMetadata> {
    let mut statement = connection
        .prepare("SELECT type, name, tbl_name, sql FROM sqlite_master")
        .unwrap();
    let mut metadata: Vec<SqliteMetadata> = statement
        .query_map([], |row| {
            Ok(SqliteMetadata {
                metadata_type: row.get(0)?,
                name: row.get(1)?,
                tbl_name: row.get(2)?,
                sql: normalize_sql(&row.get::<_, String>(3)?),
            })
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    metadata.sort();
    metadata
}

fn assert_schema_equal(connection: &Connection, schema: &str) {
    let pristine = Connection::open_in_memory().unwrap();
    pristine.execute_batch(schema).unwrap();
    assert_eq!(
        dump_sqlite_master(&pristine),
        dump_sqlite_master(connection)
    );
}

fn schemas() -> [&'static str; 6] {
    [
        // 0
        "",
        // 1
        r#"
        PRAGMA foreign_keys = OFF;

        CREATE TABLE Node(
            node_oid INTEGER PRIMARY KEY NOT NULL,
            node_id INTEGER NOT NULL);
        CREATE UNIQUE INDEX Node_node_id on Node(node_id);
        "#,
        // 2
        // Added Node.active
        // Changed node_id type from INTEGER to TEXT
        // New table Job
        r#"
        PRAGMA foreign_keys = ON;
    
        CREATE TABLE Node(
            node_oid INTEGER PRIMARY KEY NOT NULL,
            node_id TEXT NOT NULL,
            active BOOLEAN NOT NULL DEFAULT(1),
            something_else TEXT);
        CREATE UNIQUE INDEX Node_node_id on Node(node_id);
    
        CREATE TABLE Job(
            node_oid INTEGER NOT NULL,
            id INTEGER NOT NULL,
            FOREIGN KEY(node_oid) REFERENCES Node(node_oid));
        CREATE UNIQUE INDEX Job_node_oid on Job(node_oid, id);
        "#,
        // 3
        // Remove field something_else.  Note: this is significant because
        // Job.node_oid references table Node which must be recreated.
        r#"
        PRAGMA foreign_keys = ON;
    
        CREATE TABLE Node(
            node_oid INTEGER PRIMARY KEY NOT NULL,
            node_id TEXT NOT NULL,
            active BOOLEAN NOT NULL DEFAULT(1));
        CREATE UNIQUE INDEX Node_node_id on Node(node_id);
    
        CREATE TABLE Job(
            node_oid INTEGER NOT NULL,
            id INTEGER NOT NULL,
            FOREIGN KEY(node_oid) REFERENCES Node(node_oid));
        CREATE UNIQUE INDEX Job_node_oid on Job(node_oid, id);
        "#,
        // 4
        // Change index Node_node_id field
        // Delete index Job_node_id
        // Set user_version = 6
        r#"
        PRAGMA foreign_keys = ON;
    
        CREATE TABLE Node(
            node_oid INTEGER PRIMARY KEY NOT NULL,
            node_id TEXT NOT NULL,
            active BOOLEAN NOT NULL DEFAULT(1));
        CREATE UNIQUE INDEX Node_node_id on Node(node_oid);
    
        CREATE TABLE Job(
            node_oid INTEGER NOT NULL,
            id INTEGER NOT NULL,
            FOREIGN KEY(node_oid) REFERENCES Node(node_oid));
        CREATE UNIQUE INDEX Job_node_oid on Job(node_oid, id);
    
        PRAGMA user_version = 6;
        "#,
        // 5
        // (vs. schema[1]) - Change Node.active default from 1 to 2
        r#"
        CREATE TABLE Node(
            node_oid INTEGER PRIMARY KEY NOT NULL,
            node_id TEXT NOT NULL,
            active BOOLEAN NOT NULL DEFAULT(2));
        CREATE UNIQUE INDEX Node_node_id on Node(node_id);
        "#,
    ]
}
