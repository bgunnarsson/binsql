//! Rows handed on as they arrive, for a caller that writes them out rather
//! than keeping them. Memory is bounded by the channel the caller hands in,
//! counted in items, not bytes.

use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::error::{Error, Result};
use crate::value::{Column, ResultSet, Value};

/// One item of a streamed result: the columns, once, when they become known,
/// and then each row.
#[derive(Debug, Clone, PartialEq)]
pub enum Streamed {
    Columns(Vec<Column>),
    Row(Vec<Value>),
}

/// What a streamed run reports once its rows have all been sent.
#[derive(Debug, Clone, PartialEq)]
pub struct StreamSummary {
    pub rows: usize,
    pub rows_affected: Option<u64>,
    /// At least one row beyond the limit existed; it was not sent.
    pub truncated: bool,
    pub elapsed: Duration,
}

/// Where a drain loop puts what it reads.
pub(crate) enum Sink<'a> {
    /// Kept, for a result handed back whole.
    Keep(Vec<Vec<Value>>),
    /// Sent on, counting what went.
    Send(&'a mpsc::Sender<Streamed>, usize),
}

impl Sink<'_> {
    pub(crate) fn len(&self) -> usize {
        match self {
            Sink::Keep(rows) => rows.len(),
            Sink::Send(_, sent) => *sent,
        }
    }

    pub(crate) async fn columns(
        &mut self,
        columns: &[Column],
        cancel: &CancellationToken,
    ) -> Result<()> {
        match self {
            Sink::Keep(_) => Ok(()),
            Sink::Send(sender, _) => {
                deliver(sender, Streamed::Columns(columns.to_vec()), cancel).await
            }
        }
    }

    pub(crate) async fn row(&mut self, row: Vec<Value>, cancel: &CancellationToken) -> Result<()> {
        match self {
            Sink::Keep(rows) => rows.push(row),
            Sink::Send(sender, sent) => {
                deliver(sender, Streamed::Row(row), cancel).await?;
                *sent += 1;
            }
        }
        Ok(())
    }
}

/// Waits for room in the channel, or for the cancel. A receiver that has gone
/// away is nobody waiting for the rows, which is what a cancel says too.
async fn deliver(
    sender: &mpsc::Sender<Streamed>,
    item: Streamed,
    cancel: &CancellationToken,
) -> Result<()> {
    tokio::select! {
        sent = sender.send(item) => sent.map_err(|_| Error::Cancelled),
        () = cancel.cancelled() => Err(Error::Cancelled),
    }
}

/// What a drain loop learned besides the rows, which are in its [`Sink`].
pub(crate) struct Drained {
    pub columns: Vec<Column>,
    pub rows_affected: Option<u64>,
    pub truncated: bool,
    pub start: Instant,
}

impl Drained {
    pub(crate) fn new(start: Instant) -> Self {
        Drained {
            columns: Vec::new(),
            rows_affected: None,
            truncated: false,
            start,
        }
    }

    pub(crate) fn kept(self, sink: Sink<'_>) -> ResultSet {
        let rows = match sink {
            Sink::Keep(rows) => rows,
            Sink::Send(..) => Vec::new(),
        };
        ResultSet {
            columns: self.columns,
            rows,
            rows_affected: self.rows_affected,
            elapsed: self.start.elapsed(),
            truncated: self.truncated,
        }
    }

    pub(crate) fn summary(self, sink: &Sink<'_>) -> StreamSummary {
        StreamSummary {
            rows: sink.len(),
            rows_affected: self.rows_affected,
            truncated: self.truncated,
            elapsed: self.start.elapsed(),
        }
    }
}

/// Sends a result that was collected whole, for an adapter that cannot drain
/// into a sink.
pub(crate) async fn forward(
    result: ResultSet,
    sender: &mpsc::Sender<Streamed>,
    cancel: &CancellationToken,
) -> Result<StreamSummary> {
    let mut sink = Sink::Send(sender, 0);
    if !result.columns.is_empty() {
        sink.columns(&result.columns, cancel).await?;
    }
    for row in result.rows {
        sink.row(row, cancel).await?;
    }
    Ok(StreamSummary {
        rows: sink.len(),
        rows_affected: result.rows_affected,
        truncated: result.truncated,
        elapsed: result.elapsed,
    })
}
