//! Definitions read back from SQLite's own `sqlite_master`, which needs no
//! server.

use binsql_core::{
    Backend, DataSource, Definition, DefinitionForm, ObjectKind, ObjectRef, Session,
};

async fn open(name: &str, setup: &[&str]) -> (std::path::PathBuf, Session) {
    let path = std::env::temp_dir().join(format!("binsql-test-{name}-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    std::fs::write(&path, b"").expect("create database file");
    let session = Session::open(
        "definitions",
        DataSource {
            backend: Backend::Sqlite,
            dsn: path.display().to_string(),
            description: String::new(),
            read_only: false,
            open_on_start: false,
        },
    )
    .await
    .expect("open");
    for sql in setup {
        session.run(None, sql, None).await.expect("set up");
    }
    (path, session)
}

fn object(catalog: Option<&str>, name: &str, kind: ObjectKind) -> ObjectRef {
    ObjectRef::new(catalog.map(str::to_string), None, name, kind)
}

fn create(text: &str) -> Definition {
    Definition {
        form: DefinitionForm::Create,
        text: Some(text.to_string()),
    }
}

/// SQLite rewrites the opening `CREATE TABLE` as it stores it, and keeps the
/// rest as typed.
#[tokio::test]
async fn a_table_and_a_view_give_the_text_sqlite_stored() {
    let body = " Orders (\n  id INTEGER primary key, -- the key\n  total real\n)";
    let table = format!("create table {body}");
    let view = "CREATE VIEW big AS SELECT id FROM Orders WHERE total > 100";
    let (path, session) = open("definitions-text", &[&table, view]).await;

    let got = session
        .definition(&object(None, "Orders", ObjectKind::Table))
        .await
        .expect("table");
    assert_eq!(got, create(&format!("CREATE TABLE{body}")));
    let got = session
        .definition(&object(Some("main"), "big", ObjectKind::View))
        .await
        .expect("view");
    assert_eq!(got, create(view));

    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn a_renamed_table_gives_the_text_sqlite_stored() {
    let (path, session) = open(
        "definitions-renamed",
        &[
            "CREATE TABLE before (n INTEGER)",
            "ALTER TABLE before RENAME TO after",
        ],
    )
    .await;

    let got = session
        .definition(&object(None, "after", ObjectKind::Table))
        .await
        .expect("renamed table");
    assert_eq!(got, create("CREATE TABLE \"after\" (n INTEGER)"));

    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn a_missing_object_is_an_error() {
    let (path, session) = open("definitions-missing", &[]).await;

    let error = session
        .definition(&object(None, "nowhere", ObjectKind::Table))
        .await
        .expect_err("no such table");
    assert!(error.to_string().contains("nowhere"), "{error}");

    let _ = std::fs::remove_file(path);
}
