---
title: "No COMMIT after a cancel"
kind: task
mode: afk
status: resolved
blocked_by: []
claimed_by:
---

## Question

Build what 12 settled for this step; the contract is in 12's answer
(`12-statement-budget.md`).

- `sqlx_common::run_transaction` (`sqlx_common.rs:77`) and the SQL Server `run_transaction` (`mssql.rs:464`) check `cancel.is_cancelled()` before `COMMIT`. If it is cancelled, they roll back (best-effort) and return `Error::Cancelled`.
- Tests: a core test on SQLite with a token cancelled after the last statement and before commit; the rows are not present afterwards.

## Answer

A transaction whose token is cancelled after its last statement is rolled back
instead of committed, on SQLite, PostgreSQL, MySQL and SQL Server, and comes
back as a cancelled transaction error saying how it ended; `exec` reports it
as `cancelled` with "rolled back; nothing was kept", or "unknown" when the
batch could have committed on its own.

Built in docs/plans/2026-10-07-no-commit-after-cancel.md
(adapter/sqlx_common.rs, adapter/mssql.rs, cli/exec.rs). The correctness and
security reviews found nothing. Both noted that a cancel landing between the
check and `COMMIT` still commits, which is the limit of a cancel request;
accepted.

Assumed, not asked:
- It applies to ⌃C too: a stop asked for before `COMMIT` should not commit.
- The cancel is `Error::transaction(Error::Cancelled, RolledBack|Unknown, None)` rather than a bare `Error::Cancelled`, so `exec` can say how the transaction ended; the category is still `cancelled`.
- `exec` reads a failure with no statement as one after every statement ran, so a batch with its own `COMMIT` or MySQL DDL still says the outcome is unknown.
- Only a commit is stopped; a dry run rolls back anyway.
- The sqlx ending is a helper, `end`, tested on an in-memory SQLite pool, since nothing public can cancel between the last statement and `COMMIT` without racing `run`'s select.
- SQL Server gets the same check without a test: it has no seam a test could reach without a server.
