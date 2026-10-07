---
title: What does `inspect -o json` give an agent today, and what does the core know that it drops?
kind: research
mode: afk
status: open
blocked_by: []
claimed_by:
---

## Question

An agent that writes SQL against a database first needs its shape. Establish
what `binsql inspect` and `binsql inspect <table>` emit in each structured
format: columns, types, nullability, defaults, primary keys, foreign keys,
indexes, views, row counts. Then establish what `binsql-core` already reads
from each of the four backends but `inspect` does not print. Name the gaps per
backend, and whether `inspect` with no table can describe every table in one
call or needs one call per table.

## Context

- `crates/binsql/src/cli/inspect.rs` and `cli/render.rs`.
- `crates/binsql-core/src/schema.rs`, `schema_cache.rs`, and the per-backend
  introspection in `crates/binsql-core/src/adapter/`.
- `demo.db` at the repo root is a SQLite database to run it against.

## Answer
