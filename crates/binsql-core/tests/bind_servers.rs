//! Binding values against the three servers, which a plain `cargo test` has
//! none of.
//!
//! Each test reads a connection string from the environment and is ignored by
//! default. Point them at throwaway databases — each creates and drops a table
//! called `binsql_bind` — and run them deliberately:
//!
//! ```sh
//! BINSQL_TEST_POSTGRES=postgres://… \
//! BINSQL_TEST_MYSQL=mysql://… \
//! BINSQL_TEST_MSSQL="server=tcp:localhost,1433;…" \
//!     cargo test -p binsql-core --test bind_servers -- --ignored
//! ```

use binsql_core::{Backend, DataSource, ResultSet, Session, Value, sql};
use tokio_util::sync::CancellationToken;

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

async fn run(session: &Session, sql: &str, params: &[Value]) -> ResultSet {
    let backend = session.backend();
    let bound = sql::bind(&sql::split(sql, backend), params, backend).expect("placeholders");
    session
        .run_bound(None, &bound[0], None, &CancellationToken::new())
        .await
        .unwrap_or_else(|error| panic!("{sql}: {error}"))
}

fn text(value: &str) -> Value {
    Value::Text(value.to_string())
}

/// Every value goes in as text, the way `--arg` hands it over, and has to land
/// as the column's own type — then be found again by a comparison that only
/// matches if it did.
async fn round_trip(session: Session, create: &str) {
    run(&session, "DROP TABLE IF EXISTS binsql_bind", &[]).await;
    run(&session, create, &[]).await;

    let name = "it's; a -- test ?";
    run(
        &session,
        "INSERT INTO binsql_bind (id, name, born, score, active) VALUES (?, ?, ?, ?, ?)",
        &[
            text("1"),
            text(name),
            text("2024-01-31"),
            text("1.5"),
            Value::Bool(true),
        ],
    )
    .await;
    run(
        &session,
        "INSERT INTO binsql_bind (id, name, born, score, active) VALUES (?, ?, ?, ?, ?)",
        &[
            Value::Int(2),
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
        ],
    )
    .await;

    let found = run(
        &session,
        "SELECT name FROM binsql_bind WHERE id = ? AND born = ? AND score > ?",
        &[text("1"), text("2024-01-31"), Value::Float(1.0)],
    )
    .await;
    assert_eq!(found.rows, [[text(name)]]);

    let nulls = run(
        &session,
        "SELECT count(*) FROM binsql_bind WHERE name IS NULL AND born IS NULL AND id = ?",
        &[Value::Int(2)],
    )
    .await;
    assert_eq!(nulls.rows[0][0].to_text(), "1");

    run(&session, "DROP TABLE binsql_bind", &[]).await;
}

#[tokio::test]
#[ignore = "needs a Postgres server in BINSQL_TEST_POSTGRES"]
async fn postgres_converts_each_value_to_what_its_placeholder_takes() {
    let session = open("BINSQL_TEST_POSTGRES", Backend::Postgres).await;
    round_trip(
        session,
        "CREATE TABLE binsql_bind (id integer PRIMARY KEY, name text, born date, \
         score numeric(5,2), active boolean)",
    )
    .await;
}

#[tokio::test]
#[ignore = "needs a MySQL server in BINSQL_TEST_MYSQL"]
async fn mysql_binds_values_as_given() {
    let session = open("BINSQL_TEST_MYSQL", Backend::MySql).await;
    round_trip(
        session,
        "CREATE TABLE binsql_bind (id int PRIMARY KEY, name varchar(100), born date, \
         score decimal(5,2), active boolean)",
    )
    .await;
}

#[tokio::test]
#[ignore = "needs a SQL Server in BINSQL_TEST_MSSQL"]
async fn sql_server_binds_values_as_given() {
    let session = open("BINSQL_TEST_MSSQL", Backend::MsSql).await;
    round_trip(
        session,
        "CREATE TABLE binsql_bind (id int PRIMARY KEY, name nvarchar(100), born date, \
         score decimal(5,2), active bit)",
    )
    .await;
}
