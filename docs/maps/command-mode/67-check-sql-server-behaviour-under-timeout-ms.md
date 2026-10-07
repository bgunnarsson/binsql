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

Partial, 2026-10-07. Still open.

Checked on `eimskip/local` (Azure SQL, read-only source): `query "WAITFOR DELAY '00:00:05'"` is refused before it is sent (exit 2, category `refused`). The lexer classes `WAITFOR` as unknown, and an unknown statement counts as mutating. So a live `WAITFOR` test has to go through `exec` or `--allow-write`.

Unchecked: every test above. The only SQL Server sources configured are shared Azure databases flagged read-only, so nothing has been run on them that:

- writes,
- opens a transaction,
- holds a lock,
- or loads the server.

These tests need a scratch SQL Server database, or explicit permission to run them on `eimskip/local`.
