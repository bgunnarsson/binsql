---
title: "PostgreSQL and MySQL stop the server statement inside a transaction"
date: 2026-10-07
status: active
---

## Context

Ticket [64](../maps/command-mode/64-postgresql-and-mysql-stop-the-server-statement-ins.md)
builds the part of 12's contract that the transaction path is missing. A
single statement on PostgreSQL or MySQL that is cancelled has the server told
to stop it (`pg_cancel_backend`, `KILL QUERY`); a statement inside `--tx` only
has its stream dropped, so the server keeps running it and the rollback that
follows waits for it to finish. Outcome: a cancel inside a transaction stops
the running statement on the server before the rollback, as the
single-statement path does.

Settled: unattended loop; work stays on `main`, each task committed and pushed;
no AI mentions in commits. Additions only: output, messages and exit codes stay
as they are.

## Assumed, not asked

- The hook is an `Interrupt` on the `Codec` the adapters already hand the
  shared code: one function that names the connection, one that stops the
  statement running on it from another. SQLite has none.
- The single-statement path moves onto the same hook, so the two paths cannot
  drift apart; what it sends is unchanged.
- The connection is named before `BEGIN`, on the connection the transaction
  then runs on, so a failed lookup cannot abort the transaction. A lookup
  that fails leaves the batch running without a server stop, as it does for a
  single statement.
- The stop is sent only for a statement the cancel interrupted, and before
  the rollback, since the rollback cannot start until the statement ends. A
  cancel that lands after the last statement has nothing running to stop.
- The live tests are ignored by default, read their servers from
  `BINSQL_TEST_POSTGRES` and `BINSQL_TEST_MYSQL` as `bind_servers.rs` does,
  and were not run here: no server is available on this machine.

## Relevant lore

None in `docs/solutions`. From the map: tests never reach `ratatui::init`;
never name a zsh variable `status`; no `tempfile` crate.

## Acceptance criteria

- With a server, a transaction running `pg_sleep(30)` / `SLEEP(30)` that is
  cancelled after half a second returns a cancelled, rolled-back transaction
  error within 2 s, and `pg_stat_activity` / `PROCESSLIST` shows the
  statement gone within 2 s.
- The SQLite transaction tests still pass; `cargo test --workspace` passes;
  clippy is clean.

## Tasks

- [x] **1. The hook.** `sqlx_common.rs`: `Interrupt`, the transaction named
  before `BEGIN` and the stop sent before the rollback, the single-statement
  path shared. `postgres.rs`, `mysql.rs`: their `Interrupt`. `sqlite.rs`:
  none. `tests/cancel_servers.rs`: the live tests.
  Verify: `cargo test --workspace`; the live tests compile.

- [ ] **2. The README.** The Cancelling section says a cancel inside a
  transaction stops the server statement too.
  Verify: `cargo test -p binsql`.
