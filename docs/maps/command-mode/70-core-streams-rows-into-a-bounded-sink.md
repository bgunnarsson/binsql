---
title: "Core streams rows into a bounded sink"
kind: task
mode: afk
status: resolved
blocked_by: []
claimed_by:
---

## Question

Build what 13 settled for this step; the contract is in 13's answer
(`13-streaming-contract.md`).

- `binsql-core/src/value.rs` (or a new `stream.rs`): `enum Streamed { Columns(Vec<Column>), Row(Vec<Value>) }`, and `StreamSummary { rows: usize, rows_affected: Option<u64>, truncated: bool, elapsed }`.
- `Adapter::stream(&self, statement, limit, sink: mpsc::Sender<Streamed>, cancel) -> Result<StreamSummary>`. The default method calls `run` and forwards, so the addition is purely additive.
- Give the drain loop a sink parameter in `sqlx_common.rs:101-187` and `mssql.rs:158-231`: `run` collects into a `Vec` exactly as today, and `stream` sends each item through `select!` with `cancel`.
- `Columns` is sent once, when columns become known. That is the first row for sqlx and Metadata for SQL Server, mirroring today's population.
- A closed receiver behaves like a cancel: the same server-stop path, returning `Error::Cancelled`.
- Add `Session::stream_bound`, which applies `guard_read_only` like `run_bound` (`session.rs:134-146`).
- Tests (SQLite): items arrive in order, and their collected rows equal `run`'s for the same SQL. `limit` stops at N with `truncated` true at N+1 rows and false at exactly N. Cancelling or dropping the receiver mid-stream returns `Cancelled`, and the session then runs another statement. All existing `run` tests pass unchanged.
- Live PostgreSQL, MySQL and SQL Server checks only where the existing harness has them; otherwise mark them unchecked in the PR.

## Answer

`binsql-core/src/stream.rs` holds `Streamed` (`Columns`, then `Row`s) and
`StreamSummary`. `Adapter::stream` sends each item through a bounded
`mpsc::Sender` as it arrives. The default method runs and forwards; SQLite,
PostgreSQL, MySQL and SQL Server override it. One drain loop per driver
fills a `Sink`: `Keep` collects for `run` as before, and `Send` waits on the
channel or the cancel token. A closed receiver is a cancel, so it takes the
same server-stop path and returns `Error::Cancelled`. `Session::stream_bound`
applies `guard_read_only` as `run_bound` does.

Built in docs/plans/2026-10-07-core-stream-sink.md (stream.rs,
adapter/mod.rs, adapter/sqlx_common.rs, adapter/{sqlite,postgres,mysql,mssql}.rs,
session.rs, tests/sqlite_stream.rs). The six SQLite tests cover order and
equality with `run`, the limit at N and N+1, a write's `rows_affected`, a
cancel and a dropped receiver mid-stream, each followed by another statement,
and the read-only refusal. Live PostgreSQL, MySQL and SQL Server unchecked:
Docker is not running here.

The correctness review found nothing. The security review found nothing
exploitable. It noted that a consumer which stops reading without dropping
the receiver holds the connection, and on SQL Server the client lock, until
cancelled, since backpressure has no timeout. That is for the consumer to
handle: 72's writer cancels and drops the receiver when a write fails.

Assumed, not asked:
- `Streamed` and the sink live in a new `stream.rs` rather than `value.rs`.
- SQL Server's plan path collects through its own `collect_kept`, since a plan is one document, not a stream.
- A statement that returns no rows sends nothing and reports `rows_affected`.
