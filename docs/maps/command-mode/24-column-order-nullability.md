---
title: Adapter column queries are ordered, decode failures are errors, and SQL Server nullability is read correctly
kind: task
mode: afk
status: open
blocked_by: []
claimed_by:
---

## Question

Build what 19 settled for this step (see `19-schema-context-contract.md`, the contract and "Build tickets"):

- Each of the four `columns` queries orders by declared position (`cid`, `attnum`, `ordinal_position`, `column_id`).
- SQL Server reads `is_nullable` as `Value::Bool` too, so `false` means NOT NULL (`mssql.rs:405`, `:542`).
- A column row that fails to decode in `columns()` returns an error instead of being skipped.
- Tests:
  - a SQLite fixture whose primary key is declared in a different column order, checking column order;
  - a unit test of the SQL Server nullable conversion for `Bool(false)`, `Bool(true)`, `Int(0)` and `Int(1)`;
  - the existing `command_mode inspect` test still passes.
- Today's describe output is unchanged except for the SQL Server bug. Assumed, not asked: fixing that bug is not a breaking change.
- No new flag, so README and HELP are untouched.

## Context

- 19's answer is the contract; 04's answer has the adapter queries and line numbers.

## Answer
