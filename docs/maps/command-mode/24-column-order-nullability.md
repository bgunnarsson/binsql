---
title: Adapter column queries are ordered, decode failures are errors, and SQL Server nullability is read correctly
kind: task
mode: afk
status: resolved
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

Built: every adapter's column list comes back in declared order (SQLite now orders by cid), a column row whose name fails to decode is an error instead of being dropped, and SQL Server reads is_nullable's BIT so NOT NULL columns say nullable false.

Plan: docs/plans/2026-10-07-column-order-nullability.md. PostgreSQL, MySQL and SQL Server already ordered by attnum, ordinal_position and column_id; only SQLite's `pragma_table_info` query gained `ORDER BY cid`. Only the name is a decode error: the other fields keep their defaults, so describe output for rows that decoded before is unchanged. Assumed, not asked: a failing name is reported through `Error::query` like the query's own failure — it is how each adapter already reports a failed column query. Tests: `adapter::mssql::tests::is_nullable_reads_bit_and_int` and `command_mode inspect_lists_columns_in_declared_order`. Unchecked: the SQL Server fix against a live server, and the decode-error path, which no driver can be made to hit in a test. Review (four focuses): no findings.
