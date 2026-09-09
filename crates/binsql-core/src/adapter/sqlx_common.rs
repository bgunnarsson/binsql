//! The parts of the three sqlx-backed adapters that do not differ.
//!
//! Only per-cell decoding is genuinely backend-specific — the type names a
//! driver reports have nothing in common between engines — so that is left to
//! each module and passed in here as a function pointer.

use std::time::Instant;

use futures_util::TryStreamExt;
use sqlx::{Column as _, Database, Either, Executor, Pool, Row, TypeInfo as _};

use crate::error::{Error, Result};
use crate::value::{Column, ResultSet, Value};

/// Runs one statement and collects at most `limit` rows.
///
/// The row cap is applied while draining the stream rather than after, so a
/// `SELECT *` against a hundred-million-row table costs the first page and
/// nothing more.
pub async fn run<DB>(
    pool: &Pool<DB>,
    sql: &str,
    limit: Option<usize>,
    decode: fn(&DB::Row, usize) -> Value,
    affected: fn(&DB::QueryResult) -> u64,
) -> Result<ResultSet>
where
    DB: Database,
    for<'c> &'c Pool<DB>: Executor<'c, Database = DB>,
{
    let start = Instant::now();
    let mut columns: Vec<Column> = Vec::new();
    let mut rows: Vec<Vec<Value>> = Vec::new();
    let mut rows_affected: Option<u64> = None;
    let mut truncated = false;

    {
        let mut stream = sqlx::raw_sql(sql).fetch_many(pool);
        while let Some(item) = stream.try_next().await.map_err(Error::query)? {
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
