//! Exercises the whole core against a real database: connect, introspect, run,
//! cancel, and refuse a write on a read-only source.

use std::time::Duration;

use binsql_core::{Backend, DataSource, Error, ObjectKind, Session, Value};
use tokio_util::sync::CancellationToken;

fn temp_database(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("binsql-test-{name}-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    // SQLite reads a zero-length file as an empty database, which is what lets
    // the adapter keep `create_if_missing` off.
    std::fs::write(&path, b"").expect("create database file");
    path
}

fn source(path: &std::path::Path, read_only: bool) -> DataSource {
    DataSource {
        backend: Backend::Sqlite,
        dsn: path.display().to_string(),
        description: String::new(),
        read_only,
        open_on_start: false,
    }
}

#[tokio::test]
async fn introspects_and_queries_a_sqlite_database() {
    let path = temp_database("roundtrip");
    let session = Session::open("test", source(&path, false))
        .await
        .expect("open session");

    session
        .run(
            None,
            "CREATE TABLE widget (id INTEGER PRIMARY KEY, name TEXT NOT NULL, price REAL)",
            None,
        )
        .await
        .expect("create table");
    session
        .run(None, "CREATE VIEW cheap AS SELECT * FROM widget", None)
        .await
        .expect("create view");
    session
        .run(
            None,
            "INSERT INTO widget (name, price) VALUES ('bolt', 1.5), ('nut', 0.5)",
            None,
        )
        .await
        .expect("insert rows");

    let catalogs = session.catalogs().await.expect("list catalogs");
    assert!(catalogs.iter().any(|c| c.name == "main" && c.is_current));

    let objects = session.objects("main", None).await.expect("list objects");
    let widget = objects
        .iter()
        .find(|o| o.name == "widget")
        .expect("widget listed");
    assert_eq!(widget.kind, ObjectKind::Table);
    assert!(
        objects
            .iter()
            .any(|o| o.name == "cheap" && o.kind == ObjectKind::View)
    );

    let columns = session.columns(widget).await.expect("describe widget");
    assert_eq!(columns.len(), 3);
    assert_eq!(columns[0].name, "id");
    assert!(columns[0].primary_key);
    assert_eq!(columns[1].nullable, Some(false));

    let result = session
        .run(None, "SELECT name, price FROM widget ORDER BY name", None)
        .await
        .expect("select rows");
    assert_eq!(result.columns.len(), 2);
    assert_eq!(result.rows.len(), 2);
    assert_eq!(result.rows[0][0], Value::Text("bolt".into()));
    assert_eq!(result.rows[0][1], Value::Float(1.5));

    // The row cap stops the fetch rather than trimming afterwards, so it has to
    // report that there was more.
    let capped = session
        .run(None, "SELECT * FROM widget", Some(1))
        .await
        .expect("capped select");
    assert_eq!(capped.rows.len(), 1);
    assert!(capped.truncated);

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn read_only_sources_refuse_writes() {
    let path = temp_database("readonly");
    let writable = Session::open("seed", source(&path, false))
        .await
        .expect("open session");
    writable
        .run(None, "CREATE TABLE t (a INTEGER)", None)
        .await
        .expect("create table");

    let guarded = Session::open("prod", source(&path, true))
        .await
        .expect("open read-only session");

    let error = guarded
        .run(None, "DELETE FROM t", None)
        .await
        .expect_err("write refused");
    assert!(error.to_string().contains("read-only"), "{error}");

    guarded
        .run(None, "SELECT * FROM t", None)
        .await
        .expect("reads still allowed");

    let _ = std::fs::remove_file(&path);
}

/// A query nobody is waiting for any more has to hand the session back in a
/// state the next query can use.
#[tokio::test]
async fn a_cancelled_query_returns_and_leaves_the_session_usable() {
    let path = temp_database("cancel");
    let session = Session::open("test", source(&path, false))
        .await
        .expect("open session");

    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        trigger.cancel();
    });

    // Long enough to still be running when the cancel lands, short enough that
    // the test does not depend on how fast the machine under it is.
    let error = session
        .run_cancellable(
            None,
            "WITH RECURSIVE counter(x) AS (\
               SELECT 1 UNION ALL SELECT x + 1 FROM counter WHERE x < 20000000\
             ) SELECT count(*) FROM counter",
            None,
            &cancel,
        )
        .await
        .expect_err("cancelled");
    assert!(matches!(error, Error::Cancelled), "{error}");

    let result = session
        .run(None, "SELECT 1", None)
        .await
        .expect("the session still works");
    assert_eq!(result.rows[0][0], Value::Int(1));

    let _ = std::fs::remove_file(&path);
}

/// Every one of these reads as a `SELECT` — or as nothing at all — if the guard
/// only looks at the first word of the script. Each is run against a real
/// database and the table is counted afterwards, so a statement that slipped
/// through would show up as a row that is no longer there.
#[tokio::test]
async fn a_write_cannot_be_disguised_from_the_read_only_guard() {
    let path = temp_database("readonly-disguised");
    let writable = Session::open("seed", source(&path, false))
        .await
        .expect("open session");
    writable
        .run(None, "CREATE TABLE t (a INTEGER)", None)
        .await
        .expect("create table");
    writable
        .run(None, "INSERT INTO t (a) VALUES (1), (2), (3)", None)
        .await
        .expect("seed rows");

    let guarded = Session::open("prod", source(&path, true))
        .await
        .expect("open read-only session");

    let disguises = [
        "/* ticket-421 */ DELETE FROM t",
        "-- tidying up\nDELETE FROM t",
        "SELECT 1; DELETE FROM t",
        "SELECT 1;\n-- and then\nDROP TABLE t",
        "WITH doomed AS (SELECT a FROM t) DELETE FROM t WHERE a IN (SELECT a FROM doomed)",
        "   \n\tUPDATE t SET a = 0",
    ];

    for sql in disguises {
        let error = guarded
            .run(None, sql, None)
            .await
            .expect_err("refused: {sql}");
        assert!(
            error.to_string().contains("read-only"),
            "{sql} — got {error}"
        );
    }

    let survivors = guarded
        .run(None, "SELECT count(*) FROM t", None)
        .await
        .expect("count rows");
    assert_eq!(survivors.rows[0][0], Value::Int(3), "rows were written");

    // The refusal is aimed at what writes, not at everything long or unusual.
    for sql in [
        "-- yesterday's numbers\nSELECT count(*) FROM t",
        "SELECT 1; SELECT 2",
        "WITH counted AS (SELECT count(*) AS n FROM t) SELECT n FROM counted",
        "SELECT 'DELETE FROM t' AS not_a_statement",
    ] {
        guarded.run(None, sql, None).await.expect(sql);
    }

    let _ = std::fs::remove_file(&path);
}
