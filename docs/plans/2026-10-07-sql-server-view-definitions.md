---
title: "SQL Server views return their module text"
date: 2026-10-07
status: active
---

## Context

Ticket [83](../maps/command-mode/83-sql-server-views-return-their-module-text.md)
fills in SQL Server's half of 16's definitions contract on top of 80's
`Adapter::definition`, and is the last backend. Outcome: a SQL Server view
gives `Create` with `sys.sql_modules.definition`, a view whose definition
comes back null gives `Withheld`, and a table gives `Unsupported`.

## Assumed, not asked

- The object is looked up by `sys.schemas.name` and `sys.objects.name` with
  the types `objects()` lists (`U`, `V`), in the object's catalog through
  `qualify` and schema `dbo` when none is given, as `columns()` does. One
  query returns the type and, through a left join on `sys.sql_modules`, the
  module text.
- No row is `Error::Query` naming the object and schema, as PostgreSQL does.
  An object the login cannot see at all comes back as no row, so it reads the
  same as a missing one.
- The type-to-form decision is a plain function so it is unit tested without
  a server.
- The text is the module as it was written: comments before `CREATE` stay,
  `CREATE OR ALTER` stays, and a view renamed with `sp_rename` keeps its old
  name in the text. The README says so.
- A live test is added to the ignored `tests/definitions_servers.rs` and
  reads `BINSQL_TEST_MSSQL`; it is unchecked here.

## Relevant lore

- Introspection inlines names with `quote_literal` and catalogs with
  `qualify`, because a catalog cannot be a parameter.

## Acceptance criteria

- A view with module text gives `Create` and the text untouched; with a null
  gives `Withheld`; a table gives `Unsupported` with no text.
- A name that is not a table or view in the schema is an error naming it.
- The README's SQL Server row reads `unsupported | create`, its caveats say
  what the module text is, and the definitions Status bullet is gone.

## Tasks

- [ ] **1. `MsSqlAdapter::definition` and its mapping.** Override
  `definition` in `adapter/mssql.rs`; a `form_of(kind, text)` function maps
  the row. Unit tests of the mapping; an ignored live test in
  `tests/definitions_servers.rs`.
  Verify: `cargo test --workspace -q`, `cargo clippy --workspace --all-targets -q`.
- [ ] **2. README.** SQL Server row, caveat and Status bullet.
  Verify: read the table and the Status section.
