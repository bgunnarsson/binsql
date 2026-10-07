//! Rows streamed into a bounded channel, on SQLite, which needs no server.
//!
//! A file of its own, because a cancelled endless statement can leave a
//! worker thread spinning for the rest of this test binary.

use std::time::Duration;

use binsql_core::{Backend, Bound, DataSource, Error, Session, StreamSummary, Streamed};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// A recursive CTE with no end: it yields a row per step.
const ENDLESS: &str =
    "WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM r) SELECT n FROM r";

async fn open(name: &str, read_only: bool) -> (std::path::PathBuf, Session) {
    let path = std::env::temp_dir().join(format!("binsql-test-{name}-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    std::fs::write(&path, b"").expect("create database file");
    let source = |read_only| DataSource {
        backend: Backend::Sqlite,
        dsn: path.display().to_string(),
        description: String::new(),
        read_only,
        open_on_start: false,
    };
    let writer = Session::open("writer", source(false)).await.expect("open");
    for sql in [
        "CREATE TABLE t (n INTEGER, s TEXT)",
        "INSERT INTO t VALUES (1, 'a'), (2, 'b'), (3, NULL)",
    ] {
        writer.run(None, sql, None).await.expect("set up");
    }
    if !read_only {
        return (path, writer);
    }
    let session = Session::open("reader", source(true)).await.expect("open");
    (path, session)
}

/// Streams `sql` through a channel of `capacity`, collecting what arrives.
async fn stream(
    session: &Session,
    sql: &str,
    limit: Option<usize>,
    capacity: usize,
) -> (Result<StreamSummary, Error>, Vec<Streamed>) {
    let (sender, mut receiver) = mpsc::channel(capacity);
    let cancel = CancellationToken::new();
    let statement = Bound::plain(sql);
    let sent = session.stream_bound(None, &statement, limit, sender, &cancel);
    let received = async {
        let mut items = Vec::new();
        while let Some(item) = receiver.recv().await {
            items.push(item);
        }
        items
    };
    tokio::join!(sent, received)
}

#[tokio::test(flavor = "multi_thread")]
async fn streamed_items_are_the_columns_then_the_rows_run_returns() {
    let (path, session) = open("stream-same", false).await;
    let sql = "SELECT n, s FROM t ORDER BY n";
    let (summary, items) = stream(&session, sql, None, 1).await;
    let summary = summary.expect("stream");
    let result = session.run(None, sql, None).await.expect("run");

    let mut expected = vec![Streamed::Columns(result.columns.clone())];
    expected.extend(result.rows.iter().cloned().map(Streamed::Row));
    assert_eq!(items, expected);
    assert_eq!(summary.rows, 3);
    assert!(!summary.truncated);
    let _ = std::fs::remove_file(&path);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_limit_the_rows_exactly_fill_is_not_truncated() {
    let (path, session) = open("stream-limit", false).await;
    let sql = "SELECT n FROM t ORDER BY n";

    let (summary, items) = stream(&session, sql, Some(3), 8).await;
    let summary = summary.expect("stream");
    assert_eq!((summary.rows, summary.truncated), (3, false));
    assert_eq!(items.len(), 4);

    let (summary, items) = stream(&session, sql, Some(2), 8).await;
    let summary = summary.expect("stream");
    assert_eq!((summary.rows, summary.truncated), (2, true));
    assert_eq!(items.len(), 3);
    let _ = std::fs::remove_file(&path);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_statement_with_no_rows_reports_what_it_changed() {
    let (path, session) = open("stream-write", false).await;
    let (summary, items) = stream(&session, "UPDATE t SET s = 'z'", None, 8).await;
    let summary = summary.expect("stream");
    assert!(items.is_empty(), "{items:?}");
    assert_eq!(summary.rows_affected, Some(3));
    let _ = std::fs::remove_file(&path);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancel_stops_an_endless_stream() {
    let (path, session) = open("stream-cancel", false).await;
    let (sender, mut receiver) = mpsc::channel(4);
    let cancel = CancellationToken::new();
    let later = cancel.clone();
    let statement = Bound::plain(ENDLESS);
    let sent = session.stream_bound(None, &statement, None, sender, &cancel);
    let received = async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        later.cancel();
        while receiver.recv().await.is_some() {}
    };
    let (error, ()) = tokio::join!(sent, received);
    let error = error.expect_err("an endless statement does not finish");
    assert!(matches!(error, Error::Cancelled), "{error:?}");

    session
        .run(None, "SELECT count(*) FROM t", None)
        .await
        .expect("the session still runs statements");
    let _ = std::fs::remove_file(&path);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_receiver_that_goes_away_cancels_the_stream() {
    let (path, session) = open("stream-dropped", false).await;
    let (sender, mut receiver) = mpsc::channel(4);
    let cancel = CancellationToken::new();
    let statement = Bound::plain(ENDLESS);
    let sent = session.stream_bound(None, &statement, None, sender, &cancel);
    let received = async move {
        for _ in 0..10 {
            receiver.recv().await.expect("a row");
        }
    };
    let (error, ()) = tokio::join!(sent, received);
    let error = error.expect_err("an endless statement does not finish");
    assert!(matches!(error, Error::Cancelled), "{error:?}");

    session
        .run(None, "SELECT count(*) FROM t", None)
        .await
        .expect("the session still runs statements");
    let _ = std::fs::remove_file(&path);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_read_only_source_refuses_to_stream_a_write() {
    let (path, session) = open("stream-read-only", true).await;
    let (sender, _receiver) = mpsc::channel(4);
    let statement = Bound::plain("DELETE FROM t");
    let error = session
        .stream_bound(None, &statement, None, sender, &CancellationToken::new())
        .await
        .expect_err("a read-only source refuses a write");
    assert!(!matches!(error, Error::Cancelled), "{error:?}");
    let result = session
        .run(None, "SELECT count(*) FROM t", None)
        .await
        .expect("count");
    assert_eq!(result.rows[0][0], binsql_core::Value::Int(3));
    let _ = std::fs::remove_file(&path);
}
