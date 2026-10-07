---
title: "MySQL returns SHOW CREATE text"
date: 2026-10-07
status: active
---

## Context

Ticket [82](../maps/command-mode/82-mysql-returns-show-create-text.md)
fills in MySQL's half of 16's definitions contract on top of 80's
`Adapter::definition`. Outcome: a MySQL table gives `Create` with the
`Create Table` column of `SHOW CREATE TABLE`, and a view gives `Create` with
the `Create View` column of `SHOW CREATE VIEW`.

## Assumed, not asked

- The statement names the object through `ObjectRef::qualified` with the
  MySQL dialect, so the catalog is backtick-quoted in when the object has one
  and the connection's database is used when it does not.
- It is sent prepared through `sqlx::query`. MySQL lists `SHOW CREATE TABLE`
  and `SHOW CREATE VIEW` among the statements it can prepare, so 16's
  fallback to an unprepared query is not needed; a prepared statement is
  also refused if it holds more than one statement. Unchecked against a live
  server here.
- The statement and the column to read are chosen by the object's kind, as
  `objects()` set it; a function returns both so it is unit tested without a
  server.
- A missing object is the server's own error (`Table '…' doesn't exist`),
  passed on as `Error::Query`; no row and a null column give `Withheld`, the
  form for "the server gave no text".
- The live test is added to the ignored `tests/definitions_servers.rs` and
  reads `BINSQL_TEST_MYSQL`; it is unchecked here.

## Relevant lore

- `Dialect::quote_ident` doubles an embedded backtick for MySQL.

## Acceptance criteria

- A table gives `Create` with `SHOW CREATE TABLE`'s `Create Table` text; a
  view gives `Create` with `SHOW CREATE VIEW`'s `Create View` text.
- A name with a backtick and a dot is quoted as one identifier.
- The README's MySQL row reads `create | create`, its caveats say what
  MySQL's text holds, and the Status bullet no longer names MySQL.

## Tasks

- [ ] **1. `MySqlAdapter::definition`.** Override `definition` in
  `adapter/mysql.rs`; a `show_create(object)` function gives the statement
  and the column. Unit tests of quoting and of the column by kind; an ignored
  live test in `tests/definitions_servers.rs`.
  Verify: `cargo test --workspace -q`, `cargo clippy --workspace --all-targets -q`.
- [ ] **2. README.** MySQL row, caveat and Status bullet.
  Verify: read the table and the Status section.
