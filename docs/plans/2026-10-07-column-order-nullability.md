---
title: Adapter column queries are ordered, decode failures are errors, and SQL Server nullability is read correctly
date: 2026-10-07
status: in-progress
---

## Context

Map ticket `docs/maps/command-mode/24-column-order-nullability.md` builds what
ticket 19 settled (`19-schema-context-contract.md`, line 70, line 113 and the
"Build tickets" entry for 24): columns come back in declared order, a column row
that fails to decode is an error rather than a silently short list, and SQL
Server's `is_nullable` is read correctly.

What the code does today:

- SQLite `crates/binsql-core/src/adapter/sqlite.rs:128` selects from
  `pragma_table_info(?, ?)` with no `ORDER BY`. PostgreSQL
  (`postgres.rs:246`, `ORDER BY a.attnum`), MySQL (`mysql.rs:244`,
  `ORDER BY ordinal_position`) and SQL Server (`mssql.rs:391`,
  `ORDER BY c.column_id`) already order explicitly, so only SQLite's query
  changes.
- All four map rows with `filter_map` and drop a row whose name will not decode:
  `sqlite.rs:139` (`row.try_get("name").ok()?`), `postgres.rs:258`
  (`row.try_get("attname").ok()?`), `mysql.rs:256`
  (`row.try_get("name").ok()?`), `mssql.rs:403` (`row.first()?.to_text()`).
- SQL Server: `decode` turns BIT into `Value::Bool` (`mssql.rs:542`), but the
  nullable test at `mssql.rs:405` is `!matches!(v, Value::Int(0))`, so
  `Bool(false)` reads as nullable and every NOT NULL column is reported nullable.

Decisions (Assumed, not asked; the user is unavailable):

- Only the column name is a required field. A name that fails to decode (or, on
  SQL Server, a missing first cell) returns `Error::query(..)`; the other fields
  keep their current fallbacks (`unwrap_or_default`, `.ok()`), so describe output
  for every row that decodes today is unchanged. Turning those fallbacks into
  errors too would change output for odd-but-valid rows (e.g. a SQLite column
  with no declared type), which the ticket rules out.
- The SQL Server conversion becomes a named free function `nullable(&Value) ->
  bool` in `mssql.rs`, true unless the value is `Bool(false)` or `Int(0)`, so it
  can be unit-tested without a server. `Int(0)` stays recognised in case a driver
  path hands back an integer.
- Fixing the SQL Server nullability bug is not a breaking change (ticket 24).
- The column-order test is an integration test in
  `crates/binsql/tests/command_mode.rs`, beside the existing `inspect` test,
  because `sqlite.rs` has no test module and the CLI fixture already builds
  SQLite databases. SQLite has no "primary key position" separate from `cid`;
  declaring the primary key last proves the list follows declaration order, not
  key-first order.
- No decode-failure test: none of the drivers can be made to return an
  undecodable name from a real catalog query, and the change is a mechanical
  `?` in place of `.ok()?`.
- No new flag, so README and HELP are untouched.

## Relevant lore

None found.

## Acceptance criteria

- The SQLite columns query ends `ORDER BY cid`; the other three keep their
  existing `ORDER BY`.
- No adapter's `columns` uses `filter_map`; a row whose name fails to decode
  makes `columns` return `Err(Error::Query(..))`.
- SQL Server reports a NOT NULL column (`is_nullable` = `Bool(false)`) as
  `nullable: Some(false)`.
- `binsql inspect <table> -o csv` on a SQLite table whose primary key is declared
  last lists the columns in declaration order.
- `cargo test -p binsql --test command_mode inspect` and
  `cargo test -p binsql-core mssql` pass; `cargo clippy --workspace --all-targets`
  is clean.

## Tasks

- [x] SQL Server nullability. In `crates/binsql-core/src/adapter/mssql.rs` add a
  private free function near `decode`:
  `fn nullable(value: &Value) -> bool { !matches!(value, Value::Bool(false) | Value::Int(0)) }`
  with a one-line doc comment saying `is_nullable` is BIT, which `decode` reads
  as `Bool`. Change line 405 to `nullable: row.get(2).map(nullable),`. In the
  existing `mod tests` (`mssql.rs:750`) add
  `is_nullable_reads_bit_and_int` asserting `nullable(&Value::Bool(false)) ==
  false`, `Bool(true) == true`, `Int(0) == false`, `Int(1) == true`.
  Verify: `cargo test -p binsql-core mssql`.
- [ ] Decode failures are errors. In each `columns` replace
  `.iter().filter_map(|row| { ... Some(Column { .. }) })` with
  `.iter().map(|row| { ... Ok(Column { .. }) }).collect()` (the function already
  returns `Result<Vec<Column>>`, so `collect()` into it):
  - `sqlite.rs:139`: `let name: String = row.try_get("name").map_err(Error::query)?;`
  - `postgres.rs:258`: `name: row.try_get("attname").map_err(Error::query)?,`
  - `mysql.rs:256`: `name: row.try_get("name").map_err(Error::query)?,`
  - `mssql.rs:403`: `name: row.first().ok_or_else(|| Error::query(anyhow::anyhow!("a column row came back with no name")))?.to_text(),`
    (use the `anyhow!` form the file already imports, if it does; otherwise the
    fully qualified path).
  All other field expressions stay as they are.
  Verify: `cargo build -p binsql-core` and
  `cargo test -p binsql --test command_mode inspect`.
- [ ] SQLite order. In `sqlite.rs:128` make the query
  `SELECT name, type, "notnull", dflt_value, pk FROM pragma_table_info(?, ?) ORDER BY cid`.
  In `crates/binsql/tests/command_mode.rs` add, after
  `inspect_lists_tables_and_describes_one` (line 510),
  `inspect_lists_columns_in_declared_order`: `Fixture::new("inspect-order")`,
  run `exec` with
  `CREATE TABLE track (title TEXT NOT NULL, album INTEGER, id INTEGER PRIMARY KEY)`
  and `.succeeds()`, then `direct(&["inspect", "track", "-o", "csv"])`
  `.succeeds()` and `.stdout_has("column,type,nullable,default,primary_key\ntitle,TEXT,false,,false\nalbum,INTEGER,true,,false\nid,INTEGER,true,,true\n")`
  (the same multi-line `stdout_has` ordering idiom as line 188). If the CSV writer
  ends lines differently, match its line ending rather than changing the writer.
  Verify: `cargo test -p binsql --test command_mode inspect` (both inspect tests).
- [ ] Settle the ticket: write the answer into
  `docs/maps/command-mode/24-column-order-nullability.md` and set its status.
  Verify: `cargo test --workspace` and `cargo clippy --workspace --all-targets`.

## Files

- `crates/binsql-core/src/adapter/sqlite.rs`: `ORDER BY cid` on the columns
  query; `filter_map` to `map` + `?` on the name, reusing `Error::query`
  (`error.rs:47`).
- `crates/binsql-core/src/adapter/postgres.rs`: `filter_map` to `map`; name via
  `map_err(Error::query)?`. Query unchanged.
- `crates/binsql-core/src/adapter/mysql.rs`: as postgres.
- `crates/binsql-core/src/adapter/mssql.rs`: new `nullable` helper used at the
  nullable field; `filter_map` to `map` with a missing name as `Error::query`;
  unit test in the existing `mod tests`.
- `crates/binsql/tests/command_mode.rs`: new column-order test reusing `Fixture`,
  `direct`, `succeeds` and `stdout_has`.
- `docs/maps/command-mode/24-column-order-nullability.md`: the answer.

## Verification

- `cargo test -p binsql-core mssql`: the new nullable test passes.
- `cargo test -p binsql --test command_mode inspect`: the existing
  `inspect_lists_tables_and_describes_one` (output unchanged:
  `id,INTEGER,true,,true`, `name,TEXT,false,,false`) and the new order test pass.
- `cargo test --workspace` and `cargo clippy --workspace --all-targets` clean.
- SQL Server against a live server is not run here; the conversion is covered by
  the unit test, and `inspect <table>` on a table with a NOT NULL column should
  now show `false` in the nullable column.
