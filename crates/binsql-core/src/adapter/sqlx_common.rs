//! The parts of the three sqlx-backed adapters that do not differ.
//!
//! What genuinely differs between engines — reading a cell, and what it takes
//! to send a value for a placeholder — is left to each module and handed in
//! here as a [`Codec`].

use std::time::Instant;

use futures_util::TryStreamExt;
use futures_util::future::BoxFuture;
use sqlx::query::Query;
use sqlx::{Column as _, Database, Either, Executor, IntoArguments, Pool, Row, TypeInfo as _};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::error::{Error, Result, TransactionOutcome};
use crate::sql::Bound;
use crate::stream::{Drained, Sink, StreamSummary, Streamed};
use crate::value::{Column, ResultSet, Value};

/// A query with its values being bound to it, one at a time.
pub type Binding<'q, DB> = Query<'q, DB, <DB as Database>::Arguments<'q>>;

/// Adds one value to a query, or says why it cannot. It is handed the server's
/// type for the placeholder when the [`Codec`] asked to describe it first.
pub type Bind<DB> = for<'q> fn(
    Binding<'q, DB>,
    &'q Value,
    Option<&<DB as Database>::TypeInfo>,
) -> std::result::Result<Binding<'q, DB>, String>;

/// What one sqlx backend supplies that the shared code cannot know.
pub struct Codec<DB: Database> {
    /// Reads one cell. The type names drivers report have nothing in common
    /// between engines.
    pub decode: fn(&DB::Row, usize) -> Value,
    pub affected: fn(&DB::QueryResult) -> u64,
    pub bind: Bind<DB>,
    /// Whether to ask the server what each placeholder takes before binding.
    /// Only Postgres needs to: it types a parameter by the value sent for it,
    /// where the others convert a value to what the column holds.
    pub describe: bool,
    /// How to stop the server running a statement nobody is waiting for any
    /// more. SQLite runs in-process and stops when the stream is dropped.
    pub interrupt: Option<Interrupt<DB>>,
}

/// Dropping a stream only stops binsql listening: the server runs the
/// statement to its end, and holds the connection, and any transaction on it,
/// until it does. Stopping it takes naming the connection first and then
/// asking from another one, and no two servers spell either step the same way.
pub struct Interrupt<DB: Database> {
    /// Asks the connection for its id on the server, before anything that
    /// might need stopping runs on it. `None` when the server would not say.
    pub identify: for<'c> fn(&'c mut DB::Connection) -> BoxFuture<'c, Option<i64>>,
    /// Stops what the named connection is running, from another connection
    /// in the pool. Best-effort: nothing it reports would change what the
    /// caller does next.
    pub stop: for<'p> fn(&'p Pool<DB>, i64) -> BoxFuture<'p, ()>,
}

/// Runs one statement on a connection of its own, so that a cancel has a
/// connection to name to the server.
pub async fn run_alone<DB>(
    pool: &Pool<DB>,
    statement: &Bound,
    limit: Option<usize>,
    prepare: bool,
    codec: &Codec<DB>,
    cancel: &CancellationToken,
) -> Result<ResultSet>
where
    DB: Database,
    for<'c> &'c mut DB::Connection: Executor<'c, Database = DB>,
    for<'q> <DB as Database>::Arguments<'q>: IntoArguments<'q, DB>,
{
    let mut sink = Sink::Keep(Vec::new());
    let drained = drain_alone(pool, statement, limit, prepare, codec, &mut sink, cancel).await?;
    Ok(drained.kept(sink))
}

/// [`run_alone`], sending the rows on as they arrive rather than keeping them.
pub async fn stream_alone<DB>(
    pool: &Pool<DB>,
    statement: &Bound,
    limit: Option<usize>,
    codec: &Codec<DB>,
    sender: &mpsc::Sender<Streamed>,
    cancel: &CancellationToken,
) -> Result<StreamSummary>
where
    DB: Database,
    for<'c> &'c mut DB::Connection: Executor<'c, Database = DB>,
    for<'q> <DB as Database>::Arguments<'q>: IntoArguments<'q, DB>,
{
    let mut sink = Sink::Send(sender, 0);
    let drained = drain_alone(pool, statement, limit, false, codec, &mut sink, cancel).await?;
    Ok(drained.summary(&sink))
}

async fn drain_alone<DB>(
    pool: &Pool<DB>,
    statement: &Bound,
    limit: Option<usize>,
    prepare: bool,
    codec: &Codec<DB>,
    sink: &mut Sink<'_>,
    cancel: &CancellationToken,
) -> Result<Drained>
where
    DB: Database,
    for<'c> &'c mut DB::Connection: Executor<'c, Database = DB>,
    for<'q> <DB as Database>::Arguments<'q>: IntoArguments<'q, DB>,
{
    let mut connection = pool.acquire().await.map_err(Error::query)?;
    let id = identify(&mut *connection, codec).await;
    let result = drain::<DB>(
        &mut connection,
        statement,
        limit,
        prepare,
        codec,
        sink,
        cancel,
    )
    .await;
    if let Err(error) = &result {
        // Sent while the connection is still checked out, so the pool is not
        // left draining a query nobody is waiting for.
        stop(pool, error, id, codec).await;
    }
    result
}

/// One small round-trip per statement or batch, and the price of being able to
/// stop the large one that follows it.
async fn identify<DB: Database>(connection: &mut DB::Connection, codec: &Codec<DB>) -> Option<i64> {
    match &codec.interrupt {
        Some(interrupt) => (interrupt.identify)(connection).await,
        None => None,
    }
}

/// Stops the statement on the server when it was the cancel that ended it; an
/// error the server reported means it has stopped already.
async fn stop<DB: Database>(pool: &Pool<DB>, error: &Error, id: Option<i64>, codec: &Codec<DB>) {
    if matches!(error, Error::Cancelled)
        && let (Some(interrupt), Some(id)) = (&codec.interrupt, id)
    {
        (interrupt.stop)(pool, id).await;
    }
}

/// Runs several statements inside one transaction, on one connection, ending
/// it with a commit or a rollback.
///
/// Anything failing part-way through rolls the whole thing back, so a batch
/// either lands or it does not. `commit: false` is what makes a dry run a dry
/// run: everything is really executed, and then nothing is kept.
pub async fn run_transaction<DB>(
    pool: &Pool<DB>,
    statements: &[Bound],
    limit: Option<usize>,
    commit: bool,
    codec: &Codec<DB>,
    cancel: &CancellationToken,
) -> Result<Vec<ResultSet>>
where
    DB: Database,
    for<'c> &'c mut DB::Connection: Executor<'c, Database = DB>,
    for<'q> <DB as Database>::Arguments<'q>: IntoArguments<'q, DB>,
{
    let mut connection = pool.acquire().await.map_err(Error::query)?;
    let mut transaction = sqlx::Connection::begin(&mut *connection)
        .await
        .map_err(Error::query)?;
    // Named inside the transaction, because a pooling proxy can hand an
    // autocommit lookup to a server session other than the batch's. A lookup
    // that fails here means a connection the batch would fail on anyway.
    let id = identify(&mut *transaction, codec).await;
    let mut results = Vec::with_capacity(statements.len());

    for (index, statement) in statements.iter().enumerate() {
        let outcome = run::<DB>(&mut transaction, statement, limit, false, codec, cancel).await;
        match outcome {
            Ok(result) => results.push(result),
            Err(error) => {
                // The rollback cannot start until the server has finished the
                // statement, so a cancelled one is stopped first.
                stop(pool, &error, id, codec).await;
                // The statement's own error is the one worth reporting; a
                // rollback that also fails only says the connection is gone,
                // and that nothing can be said about what was kept.
                let ended = match transaction.rollback().await {
                    Ok(()) => TransactionOutcome::RolledBack,
                    Err(_) => TransactionOutcome::Unknown,
                };
                return Err(Error::transaction(error, ended, Some(index + 1)));
            }
        }
    }

    end(transaction, commit, cancel).await?;
    Ok(results)
}

/// Ends a transaction whose statements all ran. A cancel that landed after the
/// last of them still means stop, so the batch is rolled back rather than
/// committed.
async fn end<DB: Database>(
    transaction: sqlx::Transaction<'_, DB>,
    commit: bool,
    cancel: &CancellationToken,
) -> Result<()> {
    if commit && cancel.is_cancelled() {
        let ended = match transaction.rollback().await {
            Ok(()) => TransactionOutcome::RolledBack,
            Err(_) => TransactionOutcome::Unknown,
        };
        return Err(Error::transaction(Error::Cancelled, ended, None));
    }
    let ending = if commit {
        transaction.commit().await
    } else {
        transaction.rollback().await
    };
    ending
        .map_err(|error| Error::transaction(Error::query(error), TransactionOutcome::Unknown, None))
}

/// Runs one statement and collects at most `limit` rows.
///
/// The row cap is applied while draining the stream rather than after, so a
/// `SELECT *` against a hundred-million-row table costs the first page and
/// nothing more.
///
/// A statement with nothing to bind goes through `raw_sql`, which is what lets
/// the TUI send a whole script as one. One with values is prepared, since that
/// is the only way to send a value beside the SQL rather than inside it.
/// `prepare` sends one with nothing to bind as a prepared statement too, which
/// the server refuses to read as more than one statement whatever its text
/// hides from [`crate::sql::split`].
///
/// A cancel stops the drain and drops the stream, which is what sqlx asks of a
/// caller that wants out early: the pool tests the connection as it comes back
/// and discards it if the interrupted query left it mid-conversation. Stopping
/// the *server* is a separate matter, which [`run_alone`] and
/// [`run_transaction`] see to through the codec's [`Interrupt`].
pub async fn run<DB>(
    connection: &mut DB::Connection,
    statement: &Bound,
    limit: Option<usize>,
    prepare: bool,
    codec: &Codec<DB>,
    cancel: &CancellationToken,
) -> Result<ResultSet>
where
    DB: Database,
    for<'c> &'c mut DB::Connection: Executor<'c, Database = DB>,
    for<'q> <DB as Database>::Arguments<'q>: IntoArguments<'q, DB>,
{
    let mut sink = Sink::Keep(Vec::new());
    let drained = drain::<DB>(
        connection, statement, limit, prepare, codec, &mut sink, cancel,
    )
    .await?;
    Ok(drained.kept(sink))
}

/// [`run`], sending the rows on as they arrive rather than keeping them. A
/// receiver that goes away ends it as a cancel does.
pub async fn stream<DB>(
    connection: &mut DB::Connection,
    statement: &Bound,
    limit: Option<usize>,
    codec: &Codec<DB>,
    sender: &mpsc::Sender<Streamed>,
    cancel: &CancellationToken,
) -> Result<StreamSummary>
where
    DB: Database,
    for<'c> &'c mut DB::Connection: Executor<'c, Database = DB>,
    for<'q> <DB as Database>::Arguments<'q>: IntoArguments<'q, DB>,
{
    let mut sink = Sink::Send(sender, 0);
    let drained = drain::<DB>(
        connection, statement, limit, false, codec, &mut sink, cancel,
    )
    .await?;
    Ok(drained.summary(&sink))
}

async fn drain<DB>(
    connection: &mut DB::Connection,
    statement: &Bound,
    limit: Option<usize>,
    prepare: bool,
    codec: &Codec<DB>,
    sink: &mut Sink<'_>,
    cancel: &CancellationToken,
) -> Result<Drained>
where
    DB: Database,
    for<'c> &'c mut DB::Connection: Executor<'c, Database = DB>,
    for<'q> <DB as Database>::Arguments<'q>: IntoArguments<'q, DB>,
{
    let mut drained = Drained::new(Instant::now());

    let expected: Vec<DB::TypeInfo> = if codec.describe && !statement.params.is_empty() {
        let described = (&mut *connection)
            .describe(&statement.sql)
            .await
            .map_err(Error::query)?;
        match described.parameters() {
            Some(Either::Left(types)) => types.to_vec(),
            _ => Vec::new(),
        }
    } else {
        Vec::new()
    };

    {
        let mut stream = if statement.params.is_empty() && !prepare {
            sqlx::raw_sql(&statement.sql).fetch_many(connection)
        } else {
            let mut query = sqlx::query::<DB>(&statement.sql);
            for (index, value) in statement.params.iter().enumerate() {
                query = (codec.bind)(query, value, expected.get(index)).map_err(|reason| {
                    Error::query(anyhow::anyhow!("value {}: {reason}", index + 1))
                })?;
            }
            connection.fetch_many(query)
        };
        loop {
            let item = tokio::select! {
                item = stream.try_next() => item.map_err(Error::query)?,
                () = cancel.cancelled() => return Err(Error::Cancelled),
            };
            let Some(item) = item else {
                break;
            };

            match item {
                Either::Left(result) => {
                    let count = (codec.affected)(&result);
                    drained.rows_affected = Some(drained.rows_affected.unwrap_or(0) + count);
                }
                Either::Right(row) => {
                    if drained.columns.is_empty() {
                        drained.columns = row
                            .columns()
                            .iter()
                            .map(|c| Column::new(c.name(), c.type_info().name()))
                            .collect();
                        sink.columns(&drained.columns, cancel).await?;
                    }
                    if limit.is_some_and(|max| sink.len() >= max) {
                        drained.truncated = true;
                        break;
                    }
                    let values = (0..drained.columns.len())
                        .map(|i| (codec.decode)(&row, i))
                        .collect();
                    sink.row(values, cancel).await?;
                }
            }
        }
    }

    Ok(drained)
}

/// Binds a value as the type it already is. Right for SQLite and MySQL, which
/// convert a bound value to what the column holds themselves — a string sent
/// for an integer column arrives as the integer.
macro_rules! bind_as_given {
    ($query:expr, $value:expr) => {
        match $value {
            $crate::value::Value::Null => $query.bind(None::<&str>),
            $crate::value::Value::Bool(value) => $query.bind(*value),
            $crate::value::Value::Int(value) => $query.bind(*value),
            $crate::value::Value::Float(value) => $query.bind(*value),
            $crate::value::Value::Bytes(value) => $query.bind(value.as_slice()),
            $crate::value::Value::Decimal(value)
            | $crate::value::Value::Text(value)
            | $crate::value::Value::Uuid(value)
            | $crate::value::Value::Json(value)
            | $crate::value::Value::Timestamp(value) => $query.bind(value.as_str()),
        }
    };
}

/// Tries to read a cell as `$ty`, returning from the enclosing function on
/// success. A failure falls through to the next candidate, which is what makes
/// the decoders readable as an ordered list of guesses.
macro_rules! decode_as {
    ($row:expr, $idx:expr, $ty:ty, $ctor:expr) => {
        match sqlx::Row::try_get::<Option<$ty>, _>($row, $idx) {
            Ok(Some(value)) => return $ctor(value),
            Ok(None) => return $crate::value::Value::Null,
            Err(_) => {}
        }
    };
}

/// Last resort for a type the decoder does not name: text, then bytes, then an
/// honest placeholder. Never panics and never silently shows the wrong value.
macro_rules! decode_fallback {
    ($row:expr, $idx:expr, $type_name:expr) => {{
        decode_as!($row, $idx, String, $crate::value::Value::Text);
        decode_as!($row, $idx, Vec<u8>, $crate::value::Value::Bytes);
        $crate::value::Value::Text(format!("<{}>", $type_name))
    }};
}

pub(crate) use {bind_as_given, decode_as, decode_fallback};

#[cfg(test)]
mod tests {
    use sqlx::Sqlite;
    use sqlx::sqlite::{SqliteConnection, SqlitePoolOptions, SqliteQueryResult, SqliteTypeInfo};

    use super::*;

    /// One connection, so the table outlives each transaction.
    async fn pool() -> Pool<Sqlite> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("open an in-memory database");
        sqlx::raw_sql("CREATE TABLE t (id INTEGER)")
            .execute(&pool)
            .await
            .expect("create the table");
        pool
    }

    /// Inserts one row in a transaction and ends it as `run_transaction` does.
    async fn insert_and_end(
        pool: &Pool<Sqlite>,
        commit: bool,
        cancel: &CancellationToken,
    ) -> Result<()> {
        let mut transaction = pool.begin().await.expect("begin");
        sqlx::raw_sql("INSERT INTO t VALUES (1)")
            .execute(&mut *transaction)
            .await
            .expect("insert");
        end(transaction, commit, cancel).await
    }

    async fn rows(pool: &Pool<Sqlite>) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM t")
            .fetch_one(pool)
            .await
            .expect("count")
    }

    #[tokio::test]
    async fn a_cancel_after_the_last_statement_rolls_back() {
        let pool = pool().await;
        let cancel = CancellationToken::new();
        cancel.cancel();
        let error = insert_and_end(&pool, true, &cancel)
            .await
            .expect_err("a cancelled transaction does not commit");
        assert!(
            matches!(
                &error,
                Error::Transaction { error, outcome: TransactionOutcome::RolledBack, statement: None }
                    if matches!(**error, Error::Cancelled)
            ),
            "{error:?}"
        );
        assert_eq!(rows(&pool).await, 0);
    }

    #[tokio::test]
    async fn an_uncancelled_transaction_commits() {
        let pool = pool().await;
        insert_and_end(&pool, true, &CancellationToken::new())
            .await
            .expect("commit");
        assert_eq!(rows(&pool).await, 1);
    }

    /// The id `identify` hands out, and the one `stop` was last asked to stop.
    const NAMED: i64 = 7;
    static STOPPED: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

    fn identify(_: &mut SqliteConnection) -> BoxFuture<'_, Option<i64>> {
        Box::pin(async { Some(NAMED) })
    }

    fn record_stop(_: &Pool<Sqlite>, id: i64) -> BoxFuture<'_, ()> {
        Box::pin(async move { STOPPED.store(id, std::sync::atomic::Ordering::SeqCst) })
    }

    fn refuse_stop(_: &Pool<Sqlite>, _: i64) -> BoxFuture<'_, ()> {
        panic!("a statement the database ended needs no stop")
    }

    fn bind_nothing<'q>(
        query: Binding<'q, Sqlite>,
        _: &'q Value,
        _: Option<&SqliteTypeInfo>,
    ) -> std::result::Result<Binding<'q, Sqlite>, String> {
        Ok(query)
    }

    /// SQLite with a server-side stop, as Postgres and MySQL have one.
    fn codec(stop: for<'p> fn(&'p Pool<Sqlite>, i64) -> BoxFuture<'p, ()>) -> Codec<Sqlite> {
        Codec {
            decode: |_, _| Value::Null,
            affected: SqliteQueryResult::rows_affected,
            bind: bind_nothing,
            describe: false,
            interrupt: Some(Interrupt { identify, stop }),
        }
    }

    fn statement(sql: &str) -> Bound {
        Bound {
            sql: sql.to_string(),
            params: Vec::new(),
        }
    }

    #[tokio::test]
    async fn a_cancel_inside_a_transaction_stops_the_statement_before_rolling_back() {
        let pool = pool().await;
        let cancel = CancellationToken::new();
        let later = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            later.cancel();
        });
        let statements = [
            statement("INSERT INTO t VALUES (1)"),
            statement(
                "WITH RECURSIVE r(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM r) SELECT i FROM r",
            ),
        ];
        let codec = codec(record_stop);
        let ran = run_transaction(&pool, &statements, None, true, &codec, &cancel);
        let error = tokio::time::timeout(std::time::Duration::from_secs(5), ran)
            .await
            .expect("the cancel ends the batch")
            .expect_err("a cancelled batch does not commit");
        assert!(
            matches!(
                &error,
                Error::Transaction { error, outcome: TransactionOutcome::RolledBack, statement: Some(2) }
                    if matches!(**error, Error::Cancelled)
            ),
            "{error:?}"
        );
        assert_eq!(STOPPED.load(std::sync::atomic::Ordering::SeqCst), NAMED);
        assert_eq!(rows(&pool).await, 0);
    }

    #[tokio::test]
    async fn a_statement_that_fails_inside_a_transaction_is_not_stopped() {
        let pool = pool().await;
        let error = run_transaction(
            &pool,
            &[
                statement("INSERT INTO t VALUES (1)"),
                statement("SELECT * FROM missing"),
            ],
            None,
            true,
            &codec(refuse_stop),
            &CancellationToken::new(),
        )
        .await
        .expect_err("the batch fails");
        assert!(
            matches!(
                &error,
                Error::Transaction {
                    outcome: TransactionOutcome::RolledBack,
                    statement: Some(2),
                    ..
                }
            ),
            "{error:?}"
        );
        assert_eq!(rows(&pool).await, 0);
    }

    #[tokio::test]
    async fn a_cancelled_dry_run_still_just_rolls_back() {
        let pool = pool().await;
        let cancel = CancellationToken::new();
        cancel.cancel();
        insert_and_end(&pool, false, &cancel)
            .await
            .expect("a dry run ends the same either way");
        assert_eq!(rows(&pool).await, 0);
    }
}
