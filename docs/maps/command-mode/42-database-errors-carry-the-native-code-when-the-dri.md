---
title: "Database errors carry the native code when the driver types it"
kind: task
mode: afk
status: open
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
