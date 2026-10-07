//! `query --stream` — print each record as its row arrives, so memory holds a
//! channel's worth of rows rather than the whole result.
//!
//! The database side sends into a bounded channel; a blocking task writes what
//! comes out of it. When the writer falls behind, the channel fills and the
//! database waits, so a slow reader slows the query instead of growing it.

use std::io::{BufWriter, ErrorKind, Write};

use binsql_core::{Bound, Column, Error, Session, StreamSummary, Streamed};
use tokio::sync::mpsc::{self, Receiver, error::TryRecvError};
use tokio_util::sync::CancellationToken;

use super::render::{self, Format, Options};
use super::{Category, GRACE, NOTHING_CHANGED, Phase, Result, Stop, core, failed, note};

/// Rows in flight between the database and the writer.
const CHANNEL: usize = 256;
/// Bytes held before a write to stdout; flushed sooner when the channel runs
/// dry, so a slow query still shows its rows as they come.
const BUFFER: usize = 64 * 1024;

/// The formats that are a sequence of records, and so can be written one at a
/// time. Anything else needs every row before its first byte.
pub fn streams(format: Format) -> bool {
    matches!(format, Format::Jsonl | Format::Csv | Format::Tsv)
}

/// Runs `statement` and writes its rows as they arrive. Returns how many rows
/// were written, for `--require-rows`, or `None` when the reader went away
/// before they ran out.
pub async fn query(
    session: &Session,
    statement: &Bound,
    limit: Option<usize>,
    stop: &Stop,
    options: &Options,
) -> Result<Option<usize>> {
    // A child of the command's token, so a reader that went away stops the
    // query without reading as a ⌃C or a deadline.
    let cancel = stop.token().child_token();
    let (sender, receiver) = mpsc::channel(CHANNEL);
    let mut writer = {
        let options = options.clone();
        let cancel = cancel.clone();
        tokio::task::spawn_blocking(move || write(receiver, &options, &cancel))
    };

    let streamed = stop
        .run(
            GRACE,
            NOTHING_CHANGED,
            session.stream_bound(None, statement, limit, sender, &cancel),
        )
        .await;
    // A reader that stalls holds the writer in a write nothing can interrupt.
    // Once a ⌃C or the deadline has stopped the query, the writer gets a grace
    // to finish, and is then left to end with the process.
    let written = tokio::select! {
        biased;
        written = &mut writer => written
            .map_err(|error| failed(format!("writing output: {error}")).phase(Phase::Output))?,
        () = async {
            stop.token().cancelled().await;
            tokio::time::sleep(GRACE).await;
        } => return Err(streamed.err().unwrap_or_else(|| core(Error::Cancelled))),
    };

    let Written { rows, closed } = written?;
    let streamed = match streamed {
        // `binsql query --stream … | head` is a normal thing to type: the
        // cancel is the writer's own, and how many rows there were is unknown.
        Err(failure)
            if closed
                && failure.category == Category::Cancelled
                && !stop.token().is_cancelled() =>
        {
            return Ok(None);
        }
        streamed => streamed?,
    };
    if closed {
        return Ok(None);
    }
    let StreamSummary { truncated, .. } = streamed;
    if truncated && let Some(limit) = limit {
        note(
            options,
            &format!("stopped at --limit {limit}; more rows were available"),
        );
    }
    Ok(Some(rows))
}

struct Written {
    rows: usize,
    /// The reader closed the pipe before the rows ran out.
    closed: bool,
}

/// Writes records until the sender is dropped. Each record goes out whole;
/// a failed write cancels the query and drops the receiver, which is what
/// lets go of the connection.
fn write(
    mut receiver: Receiver<Streamed>,
    options: &Options,
    cancel: &CancellationToken,
) -> Result<Written> {
    let mut out = BufWriter::with_capacity(BUFFER, std::io::stdout().lock());
    let delimiter = if options.format == Format::Tsv {
        '\t'
    } else {
        ','
    };
    let mut columns: Vec<Column> = Vec::new();
    let mut rows = 0;

    let outcome = loop {
        let item = match receiver.try_recv() {
            Ok(item) => item,
            Err(TryRecvError::Empty) => {
                if let Err(error) = out.flush() {
                    break Err(error);
                }
                match receiver.blocking_recv() {
                    Some(item) => item,
                    None => break Ok(()),
                }
            }
            Err(TryRecvError::Disconnected) => break Ok(()),
        };
        let line = match item {
            Streamed::Columns(known) => {
                columns = known;
                if options.format != Format::Jsonl && options.header && !columns.is_empty() {
                    render::header_line(&columns, delimiter)
                } else {
                    continue;
                }
            }
            Streamed::Row(row) => {
                rows += 1;
                if options.format == Format::Jsonl {
                    render::jsonl_line(&columns, &row)
                } else {
                    render::separated_line(&row, delimiter)
                }
            }
        };
        if let Err(error) = out.write_all(line.as_bytes()) {
            break Err(error);
        }
    }
    .and_then(|()| out.flush());

    match outcome {
        Ok(()) => Ok(Written {
            rows,
            closed: false,
        }),
        Err(error) => {
            cancel.cancel();
            drop(receiver);
            if error.kind() == ErrorKind::BrokenPipe {
                Ok(Written { rows, closed: true })
            } else {
                Err(failed(format!("writing output: {error}"))
                    .category(Category::Io)
                    .phase(Phase::Output))
            }
        }
    }
}
