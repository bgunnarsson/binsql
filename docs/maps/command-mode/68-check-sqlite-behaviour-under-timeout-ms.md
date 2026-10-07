---
title: "Check SQLite behaviour under `--timeout-ms`"
kind: task
mode: afk
status: resolved
blocked_by: [60, 61, 63]
claimed_by:
---

## Question

Build what 12 settled for this step; the contract is in 12's answer
(`12-statement-budget.md`).

- Tests:
  - a write lock held by a second connection, with a busy wait shorter and one longer than sqlx's default `busy_timeout` (unchecked);
  - a long CPU-bound CTE: check whether the worker stops when the stream is dropped or only at process exit, and that the lock is released after exit.
- Record the results. If the step keeps running, document that the deadline is enforced by the process exiting.

## Answer

A cancelled SQLite statement that returns rows stops at its next row and
lets go of its lock. One that returns none until it ends, such as a
`count(*)` or an `INSERT … SELECT`, does not stop: sqlx's worker thread
notices a dropped stream only when it next hands over a row. So for SQLite
the deadline is enforced by the process exiting, which ends the statement
and releases its lock. `query` exits at the deadline. A transactional `exec`
exits at the deadline plus the 2 s grace, because its rollback queues behind
the worker. It says the outcome is unknown, and the uncommitted write does
not survive: the next connection rolls back the hot journal.

A wait for another connection's lock counts against the deadline. sqlx's
default `busy_timeout` of 5 s ends it first, as `database is locked`
(category `database`, code `5`), when the deadline is longer than that or
there is none. A shorter deadline ends it as a timeout. A lock released
inside the busy wait lets the waiting write through.

Built in docs/plans/2026-10-07-sqlite-under-the-deadline.md
(binsql-core/tests/sqlite_steps.rs, binsql/tests/command_mode.rs,
adapter/sqlite.rs, README). The security review found nothing. The
correctness review found four problems in the tests, all fixed:
- the released-lock test did not prove the write had waited;
- a fixed 300 ms head start raced the holder and leaked it on a panic;
- the busy wait was not timed;
- the steps tests left their database files behind.

It also found that the doc comment on `SqliteAdapter::run` was stale; fixed.

Stopping the step in-process would take `sqlite3_interrupt` through sqlx's
`lock_handle`. The TUI's ⌃C, which does not exit, would need it. Command
mode does not.

Assumed, not asked:
- These are checks, not a change: binsql's handling is left as it is, and the README says what a cancel and a deadline do on SQLite.
- The steps tests live in a file of their own, since the statement left running spins a thread for the rest of that test binary.
- The busy timeout stays at the driver's 5 s; binsql does not tie it to `--timeout-ms`.
