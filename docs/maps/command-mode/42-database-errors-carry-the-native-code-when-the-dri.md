---
title: "Database errors carry the native code when the driver types it"
kind: task
mode: afk
status: resolved
blocked_by: [40]
claimed_by:
---

## Question

Build what 10 settled for this step; the contract is in 10's answer
(`10-structured-errors.md`).

- Add a core `Error::native_code()` that walks the anyhow chain of `Error::Query` and `Error::Connect`, reading sqlx `DatabaseError::code()` (PostgreSQL, MySQL, SQLite) and tiberius's server error number (SQL Server, as a decimal string). Return `None` otherwise.
- Unchecked: that the adapters keep the driver error in the chain, and what the MySQL `code()` returns. The README lists the code's meaning per backend.
- Tests: a SQLite UNIQUE violation gives a `code` that matches the value read from the driver in the test; a binsql-side failure (`refused`) has no `code`.

## Answer

A database failure's JSON record carries `code`, the driver's own typed error code as a string — the SQLSTATE on PostgreSQL and MySQL, the extended result code on SQLite, the error number on SQL Server — read by `Error::native_code()` from the error chain, never from the message.

Built in docs/plans/2026-10-07-native-error-code.md (crates/binsql-core/src/error.rs: native_code; adapter/mssql.rs: the SHOWPLAN hint keeps the server's error as context; crates/binsql/src/cli/mod.rs: Parts' code, caused, error_record; tests/command_mode.rs; README's Structured errors; HELP's ERRORS). Checked: every adapter hands the driver error to `Error::query` or `Error::connect` whole, so it stays in the chain; sqlx's MySQL `code()` is the SQLSTATE. Text output keeps its bytes and every exit code is unchanged. The correctness and security reviews found nothing to fix.

Assumed, not asked: MySQL's code is the SQLSTATE `code()` gives (`23000`), not the server's error number (`1062`), so the field means the same on PostgreSQL and MySQL.
Assumed, not asked: SQLite's code is the extended result code (`2067` for UNIQUE), in decimal.
Assumed, not asked: the SHOWPLAN hint is added as context on the server's error, so a refused plan carries its number too; its text is unchanged.
Assumed, not asked: the code is written unmasked; it is a short code, never data.
