//! What a cancel does to a SQLite statement, which runs on a worker thread in
//! this process rather than on a server. The worker notices a dropped stream
//! only when it next hands over a row, so a statement that yields rows stops
//! and one that yields none runs on, holding its lock, until the process
//! exits. "Still running" is read through the lock: a second session's write
//! waits while the first statement holds it.
//!
//! A file of its own, because the statement left running spins a thread for
//! the rest of this test binary.

use std::time::Duration;

use binsql_core::{Backend, DataSource, Error, Session};
use tokio_util::sync::CancellationToken;

/// A recursive CTE with no end: it yields a row per step.
const ENDLESS: &str =
    "WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM r) SELECT n FROM r";

async fn open(name: &str) -> (Session, Session) {
    let path = std::env::temp_dir().join(format!("binsql-test-{name}-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    std::fs::write(&path, b"").expect("create database file");
    let source = || DataSource {
        backend: Backend::Sqlite,
        dsn: path.display().to_string(),
        description: String::new(),
        read_only: false,
        open_on_start: false,
    };
    let first = Session::open("first", source()).await.expect("open");
    first
        .run(None, "CREATE TABLE t (n INTEGER)", None)
        .await
        .expect("create table");
    let second = Session::open("second", source()).await.expect("open");
    (first, second)
}

/// Runs `sql` on `session` and cancels it a tenth of a second in.
async fn cancel_soon(session: &Session, sql: &str) {
    let cancel = CancellationToken::new();
    let later = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        later.cancel();
    });
    let error = session
        .run_cancellable(None, sql, None, &cancel)
        .await
        .expect_err("an endless statement does not finish");
    assert!(matches!(error, Error::Cancelled), "{error:?}");
}

/// Writes a row from `session`, giving up after a second.
async fn write(session: &Session) -> Result<(), Error> {
    let cancel = CancellationToken::new();
    let later = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        later.cancel();
    });
    session
        .run_cancellable(None, "INSERT INTO t VALUES (1)", None, &cancel)
        .await
        .map(|_| ())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_statement_that_yields_rows_lets_go_of_its_lock() {
    let (first, second) = open("steps-rows").await;
    cancel_soon(&first, ENDLESS).await;
    write(&second)
        .await
        .expect("the worker stopped and released its lock");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_statement_that_yields_no_rows_keeps_running() {
    let (first, second) = open("steps-no-rows").await;
    cancel_soon(
        &first,
        &format!("INSERT INTO t SELECT count(*) FROM ({ENDLESS})"),
    )
    .await;
    let error = write(&second)
        .await
        .expect_err("the worker is still running and holds the lock");
    assert!(matches!(error, Error::Cancelled), "{error:?}");
}
