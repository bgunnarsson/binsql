---
title: `inspect --columns` lists every object's columns with identity
kind: task
mode: afk
status: open
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
