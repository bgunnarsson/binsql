---
title: `inspect --columns` lists every object's columns with identity
kind: task
mode: afk
status: resolved
blocked_by: [24]
claimed_by:
---

## Question

Build what 19 settled for this step (see `19-schema-context-contract.md`, the contract and "Build tickets"):

- Adds the `columns` switch to the parse call in `inspect.rs`, along with the scope, row shape, ordering, empty-object row and all-or-nothing failure above.
- Tests:
  - a SQLite fixture with a table and a view, checked as JSON (exact envelope and rows), JSONL and CSV;
  - an object with no columns;
  - `--schema` filtering (a unit test of the CLI-side sort and filter is enough where only SQLite runs in CI);
  - today's `inspect` and `inspect <t>` output is byte-identical with and without `--columns` absent.
- README: inspect section and Status line. HELP: `--columns` in `cli/mod.rs`.

## Context

- 19's answer is the contract; 04's answer has the adapter queries and line numbers.

## Answer

Built: `inspect --columns` lists every table and view's columns in one result, one row per column with catalog, schema, object and kind (plan docs/plans/2026-10-07-inspect-columns.md).

Objects are sorted by schema (null first), name and kind, comparing bytes, and columns keep their declared order. An object with no columns is one row of nulls past its identity, with a note on stderr. Everything is read before anything prints, and a failing column query names the object and exits 1. Checked by unit tests of the sort and of the empty-object row, by the SQLite JSON, JSONL and CSV tests, and by a test that `inspect` and `inspect <name>` without `--columns` print the same bytes as before.

- Assumed, not asked: `inspect --columns <name>` exits 2 until ticket 26 matches a name exactly. Reusing today's case-insensitive, dot-splitting `find` would have promised behaviour 26 has to take back.
- Assumed, not asked: `catalog` is the catalog the call read, not `ObjectRef.catalog`. `schema` is null on SQLite and MySQL, where plain `inspect` prints "".
- Pre-existing, unfixed: database-controlled names and error text reach the terminal with control characters unescaped, in table output (render.rs) and in stderr (`failed(error.to_string())`). `--columns` adds two more stderr paths of the same kind: the column-error message and the no-columns note.
- Not checked live: schema scoping and the catalog value on PostgreSQL, MySQL and SQL Server. They are covered by the unit test and by reading the code.
- Not tested end to end: the empty-object row, which SQLite cannot easily produce. The unit test covers it.
