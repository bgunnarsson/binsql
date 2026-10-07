//! A cancel inside a transaction has to stop the statement on the server, not
//! just stop listening for it — which only a real server can show.
//!
//! Each test reads a connection string from the environment and is ignored by
//! default. Each runs a thirty-second sleep in a transaction, cancels it, and
//! looks for the sleep among what the server is running:
//!
//! ```sh
//! BINSQL_TEST_POSTGRES=postgres://… \
//! BINSQL_TEST_MYSQL=mysql://… \
//!     cargo test -p binsql-core --test cancel_servers -- --ignored
//! ```

use std::time::{Duration, Instant};

use binsql_core::{Backend, Bound, DataSource, Error, Session, TransactionOutcome};
use tokio_util::sync::CancellationToken;

/// 12's cleanup grace: what a command waits for after its deadline.
const GRACE: Duration = Duration::from_millis(2000);

async fn open(variable: &str, backend: Backend) -> Session {
    let dsn = std::env::var(variable)
        .unwrap_or_else(|_| panic!("{variable} names the database to test against"));
    let source = DataSource {
        backend,
        dsn,
        description: String::new(),
        read_only: false,
        open_on_start: false,
    };
    Session::open(variable, source).await.expect("connect")
}

fn statement(sql: &str) -> Bound {
    Bound {
        sql: sql.to_string(),
        params: Vec::new(),
    }
}

/// Runs `sleep` as the only statement of a transaction, cancels it half a
/// second in, and expects the batch rolled back within the grace and the
/// sleep gone from what `running` counts within it too.
async fn cancel_inside_a_transaction(session: Session, sleep: &str, running: &str) {
    let cancel = CancellationToken::new();
    let later = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        later.cancel();
    });

    let statements = [statement(sleep)];
    let ran = session.run_transaction(None, &statements, None, true, &cancel);
    let error = tokio::time::timeout(Duration::from_millis(500) + GRACE, ran)
        .await
        .expect("the cancel ends the batch within the grace")
        .expect_err("a cancelled batch does not commit");
    assert!(
        matches!(
            &error,
            Error::Transaction { error, outcome: TransactionOutcome::RolledBack, statement: Some(1) }
                if matches!(**error, Error::Cancelled)
        ),
        "{error:?}"
    );

    let cancelled_at = Instant::now();
    loop {
        let count = session
            .run_bound(None, &statement(running), None, &CancellationToken::new())
            .await
            .unwrap_or_else(|error| panic!("{running}: {error}"));
        if count.rows[0][0].to_text() == "0" {
            break;
        }
        assert!(
            cancelled_at.elapsed() < GRACE,
            "the server is still running the sleep"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
#[ignore = "needs a Postgres server in BINSQL_TEST_POSTGRES"]
async fn postgres_stops_a_cancelled_statement_inside_a_transaction() {
    let session = open("BINSQL_TEST_POSTGRES", Backend::Postgres).await;
    cancel_inside_a_transaction(
        session,
        "SELECT pg_sleep(30) AS binsql_stop_me",
        "SELECT count(*) FROM pg_stat_activity \
         WHERE state = 'active' AND query LIKE '%binsql_stop_me%' AND pid <> pg_backend_pid()",
    )
    .await;
}

#[tokio::test]
#[ignore = "needs a MySQL server in BINSQL_TEST_MYSQL"]
async fn mysql_stops_a_cancelled_statement_inside_a_transaction() {
    let session = open("BINSQL_TEST_MYSQL", Backend::MySql).await;
    cancel_inside_a_transaction(
        session,
        "SELECT SLEEP(30) AS binsql_stop_me",
        "SELECT COUNT(*) FROM information_schema.PROCESSLIST \
         WHERE INFO LIKE '%binsql_stop_me%' AND ID <> CONNECTION_ID()",
    )
    .await;
}
