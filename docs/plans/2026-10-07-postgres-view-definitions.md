---
title: "PostgreSQL views return their query"
date: 2026-10-07
status: done
---

## Context

Ticket [81](../maps/command-mode/81-postgresql-views-return-their-query.md)
fills in PostgreSQL's half of 16's definitions contract on top of 80's
`Adapter::definition`. Outcome: a PostgreSQL view or materialized view gives
`Query` with the text of `pg_get_viewdef(oid, true)`; a table, partitioned
table or foreign table gives `Unsupported`, as it does now.

## Assumed, not asked

- The object is looked up by `pg_namespace.nspname` and `pg_class.relname`
  with the same relkinds `objects()` lists, schema `public` when none is
  given, as `columns()` does. One query returns the relkind and, for `v` and
  `m` only, the view text.
- No row is `Error::Query` naming the object and schema, as SQLite names the
  object and catalog.
- A view whose `pg_get_viewdef` comes back null gives `Withheld`, the form 16
  defines for "the server has the object but gave no text"; the server is not
  expected to do it, but a null is not passed on as an empty `Query`.
- The relkind-to-form decision is a plain function so it can be unit tested
  without a server.
- A live test of a view and a materialized view goes in a new ignored
  `tests/definitions_servers.rs`, run like `bind_servers.rs`; it is unchecked
  here.

## Relevant lore

- `objects()` decodes `relkind` as `i8` (PostgreSQL's `"char"`), not `String`.

## Acceptance criteria

- relkind `v` or `m` with text gives `Query` and the text untouched; with a
  null gives `Withheld`; `r`, `p` and `f` give `Unsupported` with no text.
- A name that is not a table or view in the schema is an error naming it.
- The README's PostgreSQL row reads `unsupported | query`, and the Status
  bullet no longer names PostgreSQL.

## Tasks

- [x] **1. `PostgresAdapter::definition` and its mapping.** Override
  `definition` in `adapter/postgres.rs`; a `form_of(relkind, text)` function
  maps the row. Unit tests of the mapping; an ignored live test in
  `tests/definitions_servers.rs`.
  Verify: `cargo test --workspace -q`, `cargo clippy --workspace --all-targets -q`.
- [x] **2. README.** PostgreSQL row and Status bullet.
  Verify: read the table and the Status section.
