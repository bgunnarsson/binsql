//! The parts of the three sqlx-backed adapters that do not differ.
//!
//! What genuinely differs between engines — reading a cell, and what it takes
//! to send a value for a placeholder — is left to each module and handed in
//! here as a [`Codec`].

use std::time::Instant;

use futures_util::TryStreamExt;
use sqlx::query::Query;
use sqlx::{Column as _, Database, Either, Executor, IntoArguments, Pool, Row, TypeInfo as _};
use tokio_util::sync::CancellationToken;

use crate::error::{Error, Result};
use crate::sql::Bound;
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
    let mut transaction = pool.begin().await.map_err(Error::query)?;
    let mut results = Vec::with_capacity(statements.len());

    for statement in statements {
        let outcome = run::<DB>(&mut transaction, statement, limit, codec, cancel).await;
        match outcome {
            Ok(result) => results.push(result),
            Err(error) => {
                // The statement's own error is the one worth reporting; a
                // rollback that also fails only says the connection is gone.
                let _ = transaction.rollback().await;
                return Err(error);
            }
        }
    }

    if commit {
        transaction.commit().await.map_err(Error::query)?;
    } else {
        transaction.rollback().await.map_err(Error::query)?;
    }

    Ok(results)
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
///
/// A cancel stops the drain and drops the stream, which is what sqlx asks of a
/// caller that wants out early: the pool tests the connection as it comes back
/// and discards it if the interrupted query left it mid-conversation. Stopping
/// the *server* is a separate matter and belongs to each adapter, since no two
/// of them spell it the same way.
pub async fn run<DB>(
    connection: &mut DB::Connection,
    statement: &Bound,
    limit: Option<usize>,
    codec: &Codec<DB>,
    cancel: &CancellationToken,
) -> Result<ResultSet>
where
    DB: Database,
    for<'c> &'c mut DB::Connection: Executor<'c, Database = DB>,
    for<'q> <DB as Database>::Arguments<'q>: IntoArguments<'q, DB>,
{
    let start = Instant::now();
    let mut columns: Vec<Column> = Vec::new();
    let mut rows: Vec<Vec<Value>> = Vec::new();
    let mut rows_affected: Option<u64> = None;
    let mut truncated = false;

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
        let mut stream = if statement.params.is_empty() {
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
                    rows_affected = Some(rows_affected.unwrap_or(0) + count);
                }
                Either::Right(row) => {
                    if columns.is_empty() {
                        columns = row
                            .columns()
                            .iter()
                            .map(|c| Column::new(c.name(), c.type_info().name()))
                            .collect();
                    }
                    if limit.is_some_and(|max| rows.len() >= max) {
                        truncated = true;
                        break;
                    }
                    rows.push(
                        (0..columns.len())
                            .map(|i| (codec.decode)(&row, i))
                            .collect(),
                    );
                }
            }
        }
    }

    Ok(ResultSet {
        columns,
        rows,
        rows_affected,
        elapsed: start.elapsed(),
        truncated,
    })
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
