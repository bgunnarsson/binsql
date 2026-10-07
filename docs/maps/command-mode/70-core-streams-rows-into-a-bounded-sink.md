---
title: "Core streams rows into a bounded sink"
kind: task
mode: afk
status: open
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
