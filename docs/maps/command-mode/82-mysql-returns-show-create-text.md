---
title: "MySQL returns `SHOW CREATE` text"
kind: task
mode: afk
status: open
blocked_by: [80]
claimed_by:
---

## Question

Build what 16 settled for this step; the contract is in 16's answer
(`16-ddl-output.md`).

- Uses `SHOW CREATE TABLE` / `SHOW CREATE VIEW` on the quoted identifier, giving `Create`. Reads the `Create Table` or `Create View` column.
- Tests: a unit test quoting a name that contains a backtick and a dot; a unit test choosing the result column by object kind.
- Updates the README's per-backend row once 84 has added it.

## Answer
