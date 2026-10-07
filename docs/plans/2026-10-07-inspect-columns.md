---
title: inspect --columns lists every selected object's columns with its identity, in one call
date: 2026-10-07
status: in-progress
---

## Context

Ticket `docs/maps/command-mode/25-inspect-columns.md` builds the contract that
`19-schema-context-contract.md` settled. With one `inspect` call, an agent gets
every table and view in scope, one row per column, with the object's identity
beside today's five describe fields. Ticket 24 is done, so adapters now return
columns in declared order and fail on rows they cannot decode.

Decisions made here (the user asked for no questions, and we chose whatever changes least):

- **A positional with `--columns` is refused (exit 2) until ticket 26.** The
  other option was to reuse `find` (`inspect.rs:149`). We rejected it because
  `find` splits on the first dot and matches without case, which breaks 19's
  identity rule. Ticket 26 would then have to change behaviour that README, HELP
  and tests had already promised. Refusing it is one `match` arm and one test,
  and 26 replaces it. The message:
  `inspect --columns lists every object; naming one is not supported yet`.
- **The schema-scope code becomes one helper.** `list` (`inspect.rs:45-60`) and
  `find` (`inspect.rs:155-168`) contain the same code for "the named schema, or
  every schema, or `""` when the backend has no schemas". The new path needs it
  a third time, so it moves into `async fn scope(session, catalog, schema) ->
  Result<Vec<String>>`, and `list` and `find` call it. Their behaviour does not
  change.
- **Sorting and row building are a pure function** so they can be unit-tested
  without a database (only SQLite runs in CI):
  `fn column_rows(catalog: &str, objects: Vec<(ObjectRef, Vec<Column>)>) -> Vec<Vec<Value>>`.
  It sorts objects by `schema` (`Option<String>`, so `None` comes first), then
  `name`, then `kind_label`. These are plain `String` comparisons, so they are
  byte-wise and case-sensitive. Within an object, columns keep the adapter's
  order.
- **`catalog` is the catalog the call read** (the `catalog` string in `run`), not
  `ObjectRef.catalog`. On PostgreSQL, `ObjectRef.catalog` is
  `self.database.clone()` and can differ or be `None` (`postgres.rs:221`).
- **`schema` is `ObjectRef.schema` as it comes**, `None` mapped to `Value::Null`.
  SQLite and MySQL set it to `None` (`sqlite.rs:120`, `mysql.rs:226`), so those
  rows show `null`, not the `""` that today's listing prints.
- **An object with no columns** becomes one row: identity filled in, and `column`,
  `type`, `nullable`, `default` and `primary_key` all `Value::Null`. `run` then
  calls `note(&options, ...)` (`mod.rs:351`) once for each such object, after
  `print`, with the message `<schema.>object has no columns`. The exit is 0.
  `note` prints nothing in `--format none`, as it does everywhere else.
- **All or nothing:** every `objects` and `columns` call finishes before
  `print`. Any error returns `failed(...)` (exit 1) with stdout empty. A
  `columns` error names the object: `columns of <schema.>name: <error>`.

## Relevant lore

- Note (2026-10-07, mapper): `--limit` caps collected rows on the client, and
  JSONL is buffered. This plan promises no streaming, which agrees with 19
  ("partial and streaming output belongs to 13").

## Acceptance criteria

- `binsql inspect --columns` on a SQLite file with table `t` and view `v` prints
  a JSON envelope with columns `catalog, schema, object, kind, column, type,
  nullable, default, primary_key`. The rows are sorted by object, with columns
  in declared order. `schema` is `null` and `catalog` is `main`. `row_count`
  equals the number of column rows, and `truncated` is false.
- `--format jsonl` prints one row object per line with no envelope. `--format
  csv` prints a header and the same rows.
- `inspect --columns <name>` exits 2, prints the refusal on stderr and leaves
  stdout empty. `inspect --columns a b` exits 2 as well.
- Without `--columns`, the stdout of `inspect` and `inspect <t>` is
  byte-identical to today's.
- Unit tests of `column_rows` cover: objects across two schemas plus a `None`
  schema, sorted, with columns in their given order; an object with no columns
  giving one row whose last five fields are null.
- `binsql help inspect` mentions `--columns`. The README inspect section shows
  it, and the Status list has it.

## Tasks

- [ ] **`--columns` in `inspect.rs`, with tests.**
  - Pass `&["columns"]` as the switches to `parse` (`inspect.rs:15`).
  - Read the switch with `args.is_set(&["columns"])` (`args.rs:98`).
  - Refuse a positional when the switch is set.
  - Extract `scope`.
  - Add `async fn columns_of(session, catalog, schema) -> Result<(ResultSet, Vec<String>)>`.
    It returns the result and the labels of objects that had no columns. It
    loops over `scope`, then `objects`, then `columns`, and calls `column_rows`.
  - Add `column_rows`, and add `#[cfg(test)] mod tests` in `inspect.rs` with the
    two unit tests.
  - Add integration tests in `crates/binsql/tests/command_mode.rs`:
    - JSON with an exact `assert_eq!` on the parsed rows and envelope keys,
      leaving `duration_ms` out;
    - JSONL and CSV;
    - the refused positional;
    - `inspect` and `inspect t` stdout equal to fixed expected strings, written
      from today's output before the change.
  - Verify: `cargo test -p binsql inspect`, then `cargo test -p binsql --test
    command_mode inspect`, `cargo clippy -p binsql --all-targets`.
- [ ] **HELP and README.**
  - Add `--columns` to the INSPECT section in `cli/mod.rs:118-120`. It lists
    every table and view's columns with catalog, schema, object and kind.
    Naming a table with it is not supported yet.
  - In the README inspect section (~306-317), add an example and the row shape.
    Point agents to JSON or JSONL, because CSV and TSV print null and empty text
    the same way.
  - List the gaps: SQL Server `type` has no length or precision, and generated
    or hidden columns are not listed.
  - Add a Status line (~536-549).
  - Verify: `cargo test -p binsql` (the HELP tests in `mod.rs:358` stay green),
    `cargo run -p binsql -- help inspect`.

## Files

- `crates/binsql/src/cli/inspect.rs`:
  - Add the switch, the refusal, `scope` (taken out of `list` and `find`),
    `columns_of`, `column_rows` and unit tests.
  - It reuses `Session::schemas`, `objects` and `columns` (`session.rs:201-215`),
    `kind_label`, `failed`, `usage`, `note`, `print` and `render::rows`.
- `crates/binsql/tests/command_mode.rs`: add `inspect_columns_*` tests on the
  existing `Fixture`/`Run` helpers, with `assert_eq!` on `run.stdout`.
- `crates/binsql/src/cli/mod.rs`: the INSPECT help text.
- `README.md`: the inspect section and the Status list.

## Verification

```
cargo test -p binsql inspect
cargo test -p binsql --test command_mode inspect
cargo clippy -p binsql --all-targets -- -D warnings
```

Then, by hand, on a scratch SQLite file holding a table and a view:

- run `binsql inspect --columns --format json`, `--format jsonl` and
  `--format csv`, and check the row order and the null `schema`;
- run `binsql inspect --columns t` and check it exits 2 with empty stdout;
- diff `binsql inspect` and `binsql inspect t` against the output of the
  previous commit.

PostgreSQL, MySQL and SQL Server are not run in CI. Their schema scoping and
`catalog` value are covered by the unit test and by reading the code, not by a
live run.
