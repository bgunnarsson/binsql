---
title: "No COMMIT after a cancel"
kind: task
mode: afk
status: open
blocked_by: []
claimed_by:
---

## Question

Build what 12 settled for this step; the contract is in 12's answer
(`12-statement-budget.md`).

- `sqlx_common::run_transaction` (`sqlx_common.rs:77`) and the SQL Server `run_transaction` (`mssql.rs:464`) check `cancel.is_cancelled()` before `COMMIT`. If it is cancelled, they roll back (best-effort) and return `Error::Cancelled`.
- Tests: a core test on SQLite with a token cancelled after the last statement and before commit; the rows are not present afterwards.

## Answer
