---
title: "SQLite under `--timeout-ms`"
date: 2026-10-07
status: active
---

## Context

Ticket [68](../maps/command-mode/68-check-sqlite-behaviour-under-timeout-ms.md)
checks what 12 left unchecked for SQLite: whether a step stops when its stream
is dropped, and how a lock wait meets the deadline. Reading sqlx 0.8.6 says:
the default `busy_timeout` is 5 s; the worker thread notices a dropped stream
only when it next sends a row, so a statement that yields rows stops, and one
that yields none (an aggregate, an `INSERT … SELECT`) runs on, holding its
lock, until the process exits. Outcome: tests that pin each of these down, and
a README that says the deadline on such a statement is kept by exiting.

Settled: unattended loop; work stays on `main`, each task committed and pushed;
no AI mentions in commits. Checks and docs only: no behaviour changes.

## Assumed, not asked

- The worker checks are core tests in a file of their own,
  `tests/sqlite_steps.rs`, since the statement left running spins a thread for
  the rest of that test binary.
- "Still running" is read through the lock: a second session's write waits
  while the first statement holds it.
- The lock checks run the built binary, so "released after exit" means a
  process exiting: one `query --allow-write` holds the lock, a second waits on
  it.
- Interrupting SQLite's worker (`sqlite3_interrupt`) is not built here; the
  ticket asks to record, and the answer names it as the way to stop the step
  inside the process, which is what the TUI would need.

## Relevant lore

None in `docs/solutions`. From the map: tests never reach `ratatui::init`;
never name a zsh variable `status`; no `tempfile` crate.

## Acceptance criteria

- Core: a cancelled endless statement that yields rows releases its lock;
  one that yields none still holds it a second later.
- CLI: a writer waiting on a lock released inside 5 s succeeds; one held past
  5 s fails as `database is locked`, category `database`, without the flag,
  and as a timeout at the deadline with `--timeout-ms 1000`; the lock is free
  once the holder has exited.
- `cargo test --workspace` passes; clippy is clean.

## Tasks

- [x] **1. The checks.** `crates/binsql-core/tests/sqlite_steps.rs`: the two
  worker tests. `crates/binsql/tests/command_mode.rs`: the lock tests, with a
  helper that starts binsql without waiting for it.
  Verify: `cargo test --workspace`.

- [x] **2. The README.** The `query` timeout paragraph and the Cancelling
  section say what SQLite does with a statement that yields no rows.
  Verify: `cargo test -p binsql`.
