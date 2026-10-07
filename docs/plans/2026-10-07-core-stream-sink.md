---
title: "Core streams rows into a bounded sink"
date: 2026-10-07
status: active
---

## Context

Ticket [70](../maps/command-mode/70-core-streams-rows-into-a-bounded-sink.md)
builds the core half of 13's streaming contract. Today every adapter drains
its driver stream into a `Vec` and hands back a `ResultSet`. Outcome:
`Adapter::stream` and `Session::stream_bound` send `Columns` once and then each
row into a bounded `mpsc::Sender` as it arrives, and return a `StreamSummary`;
`run` collects exactly as it does today.

Settled: unattended loop; work stays on `main`, each task committed and pushed;
no AI mentions in commits. Additions only: every `run` path behaves as before.

## Assumed, not asked

- The new types live in a new `stream.rs`, exported from the crate root.
- The drain loops take an internal `Sink`, which either keeps rows or sends
  them; `run` keeps, `stream` sends. One loop per driver, not two.
- A send waits on the channel and the token in one `select!`, so a full
  channel slows the reader, and a cancel or a closed receiver ends the drain as
  `Error::Cancelled`. On PostgreSQL and MySQL that goes through `run_alone`'s
  server-stop path; on SQL Server through the connection replacement a cancel
  already takes.
- The row count for `limit` counts rows sent, so `truncated` keeps its meaning:
  at least one more row existed.
- `Adapter::stream` has a default that runs and forwards, as 13 asked, though
  every adapter in the crate overrides it.

## Relevant lore

None in `docs/solutions`. From the map: tests never reach `ratatui::init`;
SQLite test files go in the temp dir, without the `tempfile` crate.

## Acceptance criteria

- On SQLite, the streamed items are `Columns` then the rows in order, equal to
  `run`'s for the same SQL.
- `limit` N on exactly N rows is not truncated; on N+1 it is, with N sent.
- Cancelling the token, or dropping the receiver, mid-stream gives
  `Error::Cancelled`, and the session runs another statement afterwards.
- `stream_bound` refuses a write on a read-only source.
- All existing tests pass unchanged; clippy is clean. Live PostgreSQL, MySQL
  and SQL Server unchecked: Docker is not running here.

## Tasks

- [x] **1. The sink.** `stream.rs`; `Adapter::stream`; the sink through
  `sqlx_common` and `mssql`'s drain loops; each adapter's `stream`;
  `Session::stream_bound`; the SQLite tests.
  Verify: `cargo test --workspace`.
