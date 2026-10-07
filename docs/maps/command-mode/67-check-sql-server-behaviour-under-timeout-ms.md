---
title: "Check SQL Server behaviour under `--timeout-ms`"
kind: task
mode: afk
status: open
blocked_by: [60, 61, 63]
claimed_by:
---

## Question

Build what 12 settled for this step; the contract is in 12's answer
(`12-statement-budget.md`).

- Live tests:
  - `WAITFOR DELAY` in query and in exec, with and without a transaction;
  - a lock wait;
  - whether `simple_query` / `query` send blocks before the stream returns (`mssql.rs:166-174`);
  - a slow reconnect after a cancel (for example a fedauth source), which must not go past the grace.
- Check that the server rolls back the open transaction after the disconnect, and record the results.

## Answer
