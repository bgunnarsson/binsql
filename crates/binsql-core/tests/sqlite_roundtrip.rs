//! Exercises the whole core against a real database: connect, introspect, run,
//! and refuse a write on a read-only source.

use binsql_core::{Backend, DataSource, ObjectKind, Session, Value};

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
