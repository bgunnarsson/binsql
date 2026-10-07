---
title: "Core reads object definitions, with SQLite native text"
kind: task
mode: afk
status: open
blocked_by: []
claimed_by:
---

## Question

Build what 16 settled for this step; the contract is in 16's answer
(`16-ddl-output.md`).

- Adds `Definition`, `DefinitionForm`, `Adapter::definition` and `Session::definition`.
- SQLite implements it from `<catalog>.sqlite_master.sql`. PostgreSQL, MySQL and SQL Server return `Unsupported` until 81–83 land.
- Tests, against SQLite fixtures:
  - a table and a view, both `Create`, with the exact stored text;
  - an attached second catalog;
  - a table renamed with `ALTER TABLE … RENAME`, checking the stored text is returned untouched;
  - a missing object returns an error, not `Withheld`.
- No flag, no README or HELP change.

## Answer
