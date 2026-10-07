---
title: "Database errors carry the native code when the driver types it"
date: 2026-10-07
status: active
---

## Context

Ticket [42](../maps/command-mode/42-database-errors-carry-the-native-code-when-the-dri.md)
builds the third step of [10](../maps/command-mode/10-structured-errors.md)'s
contract. Every adapter hands the driver's error to `Error::query` or
`Error::connect` whole, so the `sqlx::Error` or `tiberius::error::Error` is
still in the anyhow chain. Outcome: the JSON error record carries `code`,
the database's own error code as a string, after `reason` and before `hint`,
whenever the driver gives one as a typed field. Text output is unchanged.

Settled: unattended loop; work stays on `main`, each task committed and pushed;
no AI mentions in commits. Additions only: text output stays byte-for-byte the
same, and no exit code changes.

## Assumed, not asked

- MySQL's code is sqlx's `DatabaseError::code()`, which for MySQL is the
  SQLSTATE (`23000`), not the server's error number (`1062`): one field means
  the same thing on PostgreSQL and MySQL, and the ticket names `code()`.
- SQLite's code is the extended result code sqlx reads (`2067` for a UNIQUE
  violation), as a decimal string.
- SQL Server's SHOWPLAN hint keeps the server's error in the chain, as context
  on it rather than a new error built from its text, so a refused plan carries
  its number too. Its text is unchanged.
- The code is written as the driver gives it, unmasked: it is a short code,
  never data from the server.

## Relevant lore

None in `docs/solutions`. From the map: tests never reach `ratatui::init`;
never name a zsh variable `status`.

## Acceptance criteria

- `Error::native_code()` returns the first typed code in the chain of
  `Error::Query` or `Error::Connect`, and `None` for every other variant.
- A SQLite UNIQUE violation's record has a `code` equal to the code read from
  the sqlx error in the test.
- A `refused` failure's record has no `code`.
- The README says what `code` means per backend.

## Tasks

- [ ] **1. The core reads the code.**
  `error.rs`: `native_code()` walks the chain, downcasting to `sqlx::Error`
  (its database error's `code()`) and `tiberius::error::Error` (`code()`).
  `adapter/mssql.rs`: `showplan_hint` adds its hint as context.
  Verify: `cargo test -p binsql-core` — a SQLite UNIQUE violation, a
  non-database error, the SHOWPLAN text unchanged.

- [ ] **2. The record carries it.**
  `cli/mod.rs`: `Failure` gains `code`, filled in `caused`; `error_record`
  writes it after `reason`.
  Verify: `cargo test -p binsql` — the UNIQUE violation's record and a
  refusal without one.

- [ ] **3. HELP and the README.** The field table gains `code` and its
  meaning per backend.
  Verify: `cargo test -p binsql`.
