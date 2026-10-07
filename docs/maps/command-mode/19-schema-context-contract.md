---
title: What additive one-call schema context should inspect expose?
kind: research
mode: afk
status: resolved
blocked_by: [3, 4, 5]
claimed_by:
---

## Question

Settle the contract for an agent to get every selected object's columns and
identity in one inspect call, without changing existing inspect invocations or
formats. Choose the opt-in flag, scope (selected catalog/schema/object), output
structure and supported formats, deterministic ordering, identity selection
and failure/completeness reporting. Account for duplicate names across schemas,
case-sensitive names and dots in identifiers rather than silently selecting
the first match.

Decide the minimum context this feature promises: all available column fields
and catalog/schema/name/kind, then whether ordered primary keys, foreign keys
and indexes belong in its first contract or separate additions. Define unknown,
unsupported and absent metadata distinctly. Decide how to expose backend type
limits and generated/hidden columns. Recommend avoiding automatic data scans;
if counts are chosen, specify estimated versus exact and their cost explicitly.
Native DDL belongs to 16, not this response's fidelity promise.

Write only this contract, with evidence from 04 and primary metadata docs where
new acquisition is selected. Then cut small acquisition/build tickets for the
selected scope; each must preserve existing output, follow inspect's describe
boundary, and update README.md and CLI HELP for new flags. Do not implement it
in this research ticket.

## Context

- Resolved tickets 03, 04 and 05, and MAP.md's no-breaking-changes rule.
- `crates/binsql/src/cli/inspect.rs`, `cli/render.rs`, `cli/args.rs`, `cli/mod.rs`.
- `crates/binsql-core/src/schema.rs`, `value.rs`, `session.rs`, `adapter/mod.rs`
  and the four adapters; 04 distinguishes retained metadata from acquisition gaps.
- Existing 12 owns metadata deadlines; 16 owns native DDL. Avoid duplicating them.
- The map permits routine decisions without upfront grilling; flag any product
  choices that need overruling, and mark untested backend behavior unchecked.

## Answer

Inspect gets one new opt-in switch, `--columns`. It returns every selected table and view, one row per column, with each object's full identity (catalog, schema, name, kind) beside today's five column fields. The rows go through the existing result-set renderer, so every format works. Ordered primary keys, foreign keys, indexes, generated and hidden columns, and type limits are left for later additions.

### Flag and scope

- **The flag:** `--columns` is a switch on `inspect`, added to the switch list `inspect.rs:15` passes to `Args::parse` (`args.rs:25`). Without it, `inspect` behaves byte for byte as today.
- **Catalog:** the call reads one catalog, the one given by `--catalog` or else the session's current one. It reads no other catalog (`inspect.rs:25`).
- **Schema:** `--schema S` limits the call to schema S. Without it, the call covers every schema `Session::schemas` returns, or the schema-less level on SQLite and MySQL (`inspect.rs:45`).
- **Object:** `inspect --columns <name>` limits the call to one object (see Identity). More than one positional exits 2, as today (`inspect.rs:21`).
- Assumed, not asked: the flag is a switch named `--columns`, not a mode value such as `--detail=columns`. It names what it adds and is easy to remove.

### Output, every format

The call returns one `ResultSet`, rendered by `render::rows` (`render.rs:70`), with these output columns in this order:

`catalog` text, `schema` text, `object` text, `kind` text, `column` text, `type` text, `nullable` bool, `default` text, `primary_key` bool

- **JSON:** the same envelope as every other result: `columns`, `rows`, `row_count`, `truncated` (always false) and `duration_ms` (`render.rs:301`). `row_count` counts column rows. It never counts stored data.
- **JSONL:** one row object per line, with no envelope (`render.rs:329`).
- **CSV, TSV, table, vertical, Markdown, raw and none:** the same rows, as those formats print them today. As 04 noted, null and empty text look the same in CSV and TSV, so the README points agents to JSON or JSONL.
- Assumed, not asked: flat rows rather than objects with nested column arrays. This needs no new renderer, works in every format, and keeps today's envelope. A nested shape can be added later as a separate addition.

### Ordering

- Objects are sorted by `schema` (null first), then `object`, then `kind`. Sorting compares exact bytes and is case-sensitive.
- Within an object, columns keep their declared order: SQLite `cid`, PostgreSQL `attnum`, MySQL `ordinal_position`, SQL Server `column_id`. Ticket 24 makes each adapter's columns query order explicitly. Whether the PostgreSQL, MySQL and SQL Server queries already do is unchecked.
- The sort happens in the CLI, so every backend gives the same order.

### Identity

- `catalog` is the catalog the call read: the attached database on SQLite (for example `main`), the database on MySQL, PostgreSQL and SQL Server.
- `schema` is null when the backend has no schema level (SQLite, MySQL). Today's listing prints `""` there; `--columns` does not.
- `object` is the name exactly as the server spells it.
- `kind` is `table` or `view`. PostgreSQL's materialized views, partitioned tables and foreign tables still collapse into these two (04). A finer kind would be a separate addition.
- **The positional name under `--columns`:**
  - It is one exact, case-sensitive object name. It is never split on dots, so an object called `a.b` can be named. The schema comes only from `--schema`.
  - No exact match exits 1, with the "no table or view named …" message. If names match only when case is ignored, stderr lists them.
  - If several schemas hold an exact match and `--schema` was not given, the call exits 2. Stderr lists the matching schemas and says to pass `--schema`. It never picks the first one.
- Without `--columns`, describe keeps its first-dot split and its case-insensitive first match (`inspect.rs:101`, `:177`). Changing that would break existing calls.
- Assumed, not asked: ambiguity counts as a usage mistake (exit 2), not a database refusal (exit 1).

### What the first contract promises

- **Promised:** the four identity fields above, and the five fields today's describe already prints (`value.rs:78`, `inspect.rs:115`), with the same meaning.
- **`type`:** the backend's own type text, not normalised.
  - PostgreSQL (`format_type`) and MySQL (`column_type`) include length, precision and scale.
  - SQLite gives the declared type.
  - SQL Server gives only the type name, without its limits. This is a documented gap and is not promised.
- **Generated and hidden columns:** not promised. SQLite's `pragma_table_info` leaves them out (04). Switching to `table_xinfo` would change today's describe output, so these columns, and flags marking them, are a later addition. The README lists the gap.
- **Later, separate additions, not cut here:** ordered primary-key position, foreign keys, indexes, unique and check constraints, finer kinds, SQL Server type limits, comments. Each needs new acquisition in all four adapters (04, `adapter/mod.rs:32`). Each would be its own switch with its own rows, so this contract never has to change. Native DDL stays with 16.
- **No data scans:** no `COUNT(*)`, no row estimates, no sampling. The call reads only catalog metadata.

### Unknown, unsupported, absent

- **Absent** (the database says there is none): `default` null means no default; `schema` null means the backend has no schema level.
- **Unknown** (the backend cannot say): `nullable` null, as `Column.nullable: Option<bool>` already allows.
- **Unsupported** (binsql does not acquire it for this backend): never printed as false or empty. In the first contract this applies only to SQL Server type limits and to generated or hidden columns. The README lists those per backend. Later fields follow the same rule: null where a backend can't supply them, with each backend's support documented.
- **Object with no columns:** if a listed object returns no columns (for example, dropped between the listing and the column query), it is printed as one row with `column`, `type`, `nullable`, `default` and `primary_key` all null. Stderr notes it, and the exit is 0.

### Failure and completeness

- **All or nothing:** the whole result is built before anything is printed, which `print` already does.
  - Any listing or column error exits 1. Stdout stays empty, and stderr names the failing object.
  - Partial results are not promised. Partial and streaming output belongs to 13; deadlines belong to 12.
- **Exit codes:**
  - **0:** complete.
  - **1:** a connection or metadata error, or no object matched.
  - **2:** usage: an unknown flag, two positionals, or an ambiguous name.
- **Silently dropped rows:** each adapter's `columns` uses `filter_map` (SQLite: `sqlite.rs:138`). A row that fails to decode is skipped, so the column list comes back short without saying so. Ticket 24 turns that into an error. Whether the other three adapters do the same is unchecked.
- **Cost:** the call runs one column query per object. Speed on large schemas and the effect of concurrent DDL are unchecked.

### Build tickets

- **24 — Adapter column queries are ordered, and nullability is read correctly.** mode: afk, blocked_by: []
  - Each of the four `columns` queries orders by declared position (`cid`, `attnum`, `ordinal_position`, `column_id`).
  - SQL Server reads `is_nullable` as `Value::Bool` too, so `false` means NOT NULL (`mssql.rs:405`, `:542`).
  - A column row that fails to decode in `columns()` returns an error instead of being skipped.
  - Tests:
    - a SQLite fixture whose primary key is declared in a different column order, checking column order;
    - a unit test of the SQL Server nullable conversion for `Bool(false)`, `Bool(true)`, `Int(0)` and `Int(1)`;
    - the existing `command_mode inspect` test still passes.
  - Today's describe output is unchanged except for the SQL Server bug. Assumed, not asked: fixing that bug is not a breaking change.
  - No new flag, so README and HELP are untouched.
- **25 — `inspect --columns` lists every object's columns with identity.** mode: afk, blocked_by: [24]
  - Adds the `columns` switch to the parse call in `inspect.rs`, along with the scope, row shape, ordering, empty-object row and all-or-nothing failure above.
  - Tests:
    - a SQLite fixture with a table and a view, checked as JSON (exact envelope and rows), JSONL and CSV;
    - an object with no columns;
    - `--schema` filtering (a unit test of the CLI-side sort and filter is enough where only SQLite runs in CI);
    - today's `inspect` and `inspect <t>` output is byte-identical with and without `--columns` absent.
  - README: inspect section and Status line. HELP: `--columns` in `cli/mod.rs`.
- **26 — `inspect --columns <name>` selects exactly one object.** mode: afk, blocked_by: [25]
  - Exact, case-sensitive match, no dot splitting, schema only from `--schema`.
  - No match exits 1, listing near-matches that differ only in case. Ambiguity across schemas exits 2, listing the schemas.
  - Tests:
    - a SQLite object named with a dot;
    - a case-only near-match, giving exit 1 and the stderr hint;
    - a unit test of ambiguity across two schemas on the matching function, giving exit 2 and empty stdout;
    - plain `inspect <name>` keeps today's case-insensitive behaviour and its dot split.
  - README and HELP document the positional form under `--columns`.
