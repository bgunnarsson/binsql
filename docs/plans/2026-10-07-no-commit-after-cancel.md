---
title: "No COMMIT after a cancel"
date: 2026-10-07
status: active
---

## Context

Ticket [63](../maps/command-mode/63-no-commit-after-cancel.md) is the core
step of 12's statement budget. Today a cancel that lands after the last
statement of a transaction is never looked at: `run_transaction` sends
`COMMIT` with no regard to the token, on every backend. Outcome: a cancelled
token stops the commit, the transaction is rolled back, and the caller hears
it was cancelled.

Settled: unattended loop; work stays on `main`, each task committed and pushed;
no AI mentions in commits. No flags, so HELP and the README stay as they are.

## Assumed, not asked

- It applies to ⌃C too, as 12 settled: a stop asked for before `COMMIT`
  should not commit.
- The cancel comes back as `Error::transaction(Error::Cancelled, outcome,
  None)` rather than a bare `Error::Cancelled`, so `exec` can say how the
  transaction ended; the outcome is `RolledBack` when the rollback answered
  and `Unknown` when it did not. The category is still `cancelled`.
- `exec` reads a failure with no statement as one that came after every
  statement ran (`unwrap_or(statements.len())`), so a MySQL batch with DDL, or
  any batch with its own `COMMIT`, still says the outcome is unknown rather
  than "nothing was kept". The only other failure with no statement, a failed
  commit, is already `Unknown`.
- Only a commit is stopped. A dry run rolls back anyway, so its cancel after
  the last statement changes nothing.
- The sqlx ending moves into a helper, `end`, and the test drives it on an
  in-memory SQLite pool: nothing public can cancel the token after the last
  statement and before `COMMIT` without racing `run`'s select, which picks at
  random between a ready stream and a cancelled token.
- SQL Server gets the same check, rolling back with a fresh token as its error
  path does, without a test: it has no seam a test could reach without a
  server.

## Relevant lore

None in `docs/solutions`. From the map: tests never reach `ratatui::init`.

## Acceptance criteria

- On SQLite, a transaction with a row inserted, ended with `commit` and a
  cancelled token, returns a cancelled transaction error that was rolled
  back, and the row is not there afterwards.
- The same with a live token commits the row.
- A dry run with a cancelled token rolls back and returns `Ok`.
- `cargo test --workspace` passes unchanged otherwise.

## Tasks

- [ ] **1. Check the token before COMMIT.** `adapter/sqlx_common.rs`: `end`,
  with its tests. `adapter/mssql.rs`: the same check before `COMMIT`.
  `cli/exec.rs`: a failure with no statement counts every statement as run.
  Verify: `cargo test --workspace`, `cargo clippy --workspace --all-targets`.
