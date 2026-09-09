//! The parts of the three sqlx-backed adapters that do not differ.
//!
//! Only per-cell decoding is genuinely backend-specific — the type names a
//! driver reports have nothing in common between engines — so that is left to
//! each module and passed in here as a function pointer.

use std::time::Instant;

use futures_util::TryStreamExt;
use sqlx::{Column as _, Database, Either, Executor, Pool, Row, TypeInfo as _};
use tokio_util::sync::CancellationToken;

use crate::error::{Error, Result};
use crate::value::{Column, ResultSet, Value};

/// Runs several statements inside one transaction, on one connection, ending
/// it with a commit or a rollback.
///
/// Anything failing part-way through rolls the whole thing back, so a batch
/// either lands or it does not. `commit: false` is what makes a dry run a dry
/// run: everything is really executed, and then nothing is kept.
pub async fn run_transaction<DB>(
    pool: &Pool<DB>,
    statements: &[String],
    limit: Option<usize>,
    commit: bool,
    decode: fn(&DB::Row, usize) -> Value,
    affected: fn(&DB::QueryResult) -> u64,
    cancel: &CancellationToken,
) -> Result<Vec<ResultSet>>
where
    DB: Database,
    for<'c> &'c mut DB::Connection: Executor<'c, Database = DB>,
{
    let mut transaction = pool.begin().await.map_err(Error::query)?;
    let mut results = Vec::with_capacity(statements.len());

    for sql in statements {
        let outcome = run::<DB>(&mut transaction, sql, limit, decode, affected, cancel).await;
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
/// A cancel stops the drain and drops the stream, which is what sqlx asks of a
/// caller that wants out early: the pool tests the connection as it comes back
/// and discards it if the interrupted query left it mid-conversation. Stopping
/// the *server* is a separate matter and belongs to each adapter, since no two
/// of them spell it the same way.
pub async fn run<DB>(
    connection: &mut DB::Connection,
    sql: &str,
    limit: Option<usize>,
    decode: fn(&DB::Row, usize) -> Value,
    affected: fn(&DB::QueryResult) -> u64,
    cancel: &CancellationToken,
) -> Result<ResultSet>
where
    DB: Database,
    for<'c> &'c mut DB::Connection: Executor<'c, Database = DB>,
{
    let start = Instant::now();
    let mut columns: Vec<Column> = Vec::new();
    let mut rows: Vec<Vec<Value>> = Vec::new();
    let mut rows_affected: Option<u64> = None;
    let mut truncated = false;

    {
        let mut stream = sqlx::raw_sql(sql).fetch_many(connection);
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
                    let count = affected(&result);
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
                    rows.push((0..columns.len()).map(|i| decode(&row, i)).collect());
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

pub(crate) use {decode_as, decode_fallback};
