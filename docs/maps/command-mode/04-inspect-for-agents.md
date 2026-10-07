---
title: What does `inspect -o json` give an agent today, and what does the core know that it drops?
kind: research
mode: afk
status: resolved
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

Inspect JSON gives an object list or one object's five column fields, not a full schema; the CLI drops identity and catalog context, while relationships, indexes, data row counts and definitions need new core introspection.

### Checked contract

Read tickets 03 and 05, README.md:212 and :302, and the CLI/core paths below.
The output path is shared by all four backends (`crates/binsql/src/cli/inspect.rs:34`,
`:39`); backend differences are in acquisition, not the JSON schema.

| Invocation | Rows and types in every structured format |
| --- | --- |
| `inspect` | `schema` text (empty string without a schema level), `name` text, `kind` text (`table` or `view`). No column details. |
| `inspect <object>` | `column` text, `type` text, `nullable` boolean or null, `default` string or null, `primary_key` boolean. Works for tables and views. No object identity in the response. |

The rows are constructed at `cli/inspect.rs:70` and :112. JSON is an object
with `columns`, `rows`, `row_count`, `truncated`, `duration_ms`
(`cli/render.rs:301`). Its `columns` describes the **output fields**, not the
database table: e.g. `nullable` has output type `bool`, while database types
are strings in `rows[].type`. `rows_affected` is absent; inspect sets it to
None. `row_count` counts listed objects or described columns, never the rows
stored in a table. `truncated` is always false here. Timing starts after
connecting and excludes connection/credential resolution (`inspect.rs:24`, :44,
:100). `--pretty` changes JSON formatting, not its schema.

JSONL/NDJSON emits one typed row object per line, with no envelope, column
metadata, count, timing or truncation record (`cli/render.rs:329`). CSV/TSV
emits the same fields with headers unless `--no-header`; booleans become
`true`/`false`, null becomes an empty field, indistinguishable from an empty
string (`cli/render.rs:274`, `crates/binsql-core/src/value.rs:38`). Defaults
are backend strings, not evaluated JSON values. Table/vertical/Markdown/raw
also render these same result rows; none adds relationships. `-o none`
suppresses output (`cli/render.rs:71`). JSON and JSONL preserve null versus
empty text and are preferable for agents.

No-table inspect walks every schema returned by the adapter in **one selected
catalog**, or passes no schema when that level is absent (`inspect.rs:43`).
It never describes all objects' columns. Today an agent needs one listing
plus one process invocation per object to collect the available column metadata;
two positional objects fail with usage exit 2 (`inspect.rs:18`). Each process
connects again. `--catalog` selects a database and `--schema` filters schemas on PostgreSQL/SQL Server (SQLite/MySQL adapters
ignore this argument);
it is not an all-catalog traversal. Session metadata methods open a sibling
PostgreSQL connection when needed (`crates/binsql-core/src/session.rs:201`,
:208, :215), whereas the other adapters can use the same connection.

### What is dropped, and where

Paths `adapter/...`, `schema.rs`, `value.rs` and `schema_cache.rs` below are
under `crates/binsql-core/src/`; `cli/...` is under `crates/binsql/src/`.

**Retained by core, omitted by CLI:** `ObjectRef` carries catalog, schema,
name and kind (`schema.rs:30`); listing omits catalog, and describing discards
all four after lookup. Catalog name/current flags are available through
`Session::catalogs` (`session.rs:197`, `schema.rs:23`) but inspect never calls
it. The session also exposes backend and server version (`session.rs:54`,
:62), which are absent from inspect's response. All five retained `Column`
fields are printed (`value.rs:78`, `cli/inspect.rs:115`): there is no hidden
richer column model waiting for a renderer.

**Not retained as schema metadata:** the Adapter trait offers catalogs,
schemas, objects and columns, but no foreign-key, general index, data-count,
constraint or definition method (`adapter/mod.rs:32`). SchemaCache has only
those four levels (`schema_cache.rs:33`) and generic serialized values
(:165); it does not acquire extra metadata. Its use is in the app
(`crates/binsql/src/app/mod.rs:573`), not CLI inspect. Querying catalog tables
manually through `query` is possible, but that is not a built-in schema
inventory or evidence that inspect already knows their contents.

| Backend and acquisition | Information lost before the CLI / absent from the core |
| --- | --- |
| SQLite: objects from selected `sqlite_master`, excluding `sqlite_%` (`adapter/sqlite.rs:95`); columns from `pragma_table_info` (:125), declared type, inverse `notnull`, default text, `pk > 0`. | Numeric PK position is fetched but reduced to a boolean, losing composite-key order. Generated and hidden columns are not read; effective nullability of an INTEGER PRIMARY KEY is not inferred. No FKs, general indexes, uniqueness/checks, data counts or CREATE/view SQL are acquired. Catalog identity is retained and then omitted by CLI. |
| PostgreSQL: user schemas from `pg_namespace` (`adapter/postgres.rs:183`); `pg_class` kinds r/p/v/m/f (:199); columns from `pg_attribute`, `format_type`, `pg_get_expr` and a primary-index membership join (:231). | Materialized versus ordinary views and partitioned/foreign versus ordinary tables are read as relkind but collapsed into two ObjectKind values. Type modifiers survive in formatted type text. PK membership survives, but index/key names and ordered key definitions are not selected. No FK/general-index inventory, data estimates/counts or complete definitions; generation/identity status and comments are not modeled. Object catalog/schema/kind are lost by CLI describe. |
| MySQL: catalog list from `information_schema.schemata` (`adapter/mysql.rs:172`), no separate schema level (:197); objects from `information_schema.tables` (:202); columns use `column_type`, nullable/default, `column_key` (:231). | `column_key` is fetched but only `PRI` survives; `UNI`/`MUL` hints are discarded (these alone would not describe an index). Full column_type survives, including declared modifiers. No ordered PK/constraint definitions, FKs, index inventory, data counts, generation/auto-increment status, comments or SHOW CREATE definitions. Selected database identity is omitted by CLI. |
| SQL Server: online catalogs from `sys.databases` (`adapter/mssql.rs:280`), filtered schemas (:302), U/V objects (:323); columns join sys.columns/types/default_constraints and primary-index membership (:359). | Nullability is read but incorrectly converted: a false bit becomes nullable true (see below). Only type name is selected: length, precision and scale are not acquired. sys.indexes/index_columns are used solely for PK membership, not an index inventory or ordered key. Identity/computed status, FKs, general indexes, constraint names, data counts and object/view definitions are absent. Selected catalog/schema/kind are omitted by CLI describe. |

All four recognize views in listing and pass them to the same columns method;
none fetches view source text. Object and column order follows adapter queries,
not an explicit portable PK order. System filtering and server permissions mean
“every table” is limited to what those queries return, not a server-wide dump.

SQLite's [table_info documentation](https://www.sqlite.org/pragma.html#pragma_table_info)
confirms that generated/hidden columns require table_xinfo and that pk numbers
encode key position. Its [PRIMARY KEY documentation](https://www.sqlite.org/lang_createtable.html#the_primary_key)
explains why a PK flag cannot universally imply NOT NULL. These documents
support the acquisition limits, not a proposed change to today's output.

SQL Server has an established conversion defect by code inspection:
[sys.columns documents is_nullable as bit](https://learn.microsoft.com/en-us/sql/relational-databases/system-catalog-views/sys-columns-transact-sql?view=sql-server-ver17).
`adapter/mssql.rs:542` decodes bit as Value::Bool; :405 checks only Value::Int(0)
as non-nullable. Value::Bool(false) therefore becomes nullable true. This loses
NOT NULL metadata in core before inspect renders it. Live server reproduction
remains unchecked, but is not needed to establish that conversion mismatch.

### Runtime evidence and unchecked parts

On 2026-10-07 ran `cargo test -p binsql --test command_mode inspect`: the one
inspect integration test passed. Ran `target/debug/binsql inspect --dsn
<absolute-demo.db> --driver sqlite -o json`, and described `album`, with an
isolated temporary BINSQL_CONFIG, empty BINSQL_PROJECT, connection environment
variables removed and schema cache disabled. The root demo.db was only read.
Listing returned album, artist and artist_albums (view), row_count 3. Album
returned five columns, row_count 5; id was INTEGER, primary_key true,
nullable true; default values were null. Stdout was JSON, stderr empty, exit 0.

An isolated temporary SQLite fixture had parent(a,b, PRIMARY KEY(b,a)), child
with INTEGER PRIMARY KEY, numeric/text defaults, a generated stored column,
a foreign key, an index, one data row, and a view. Child's JSON/JSONL/CSV/TSV
all returned only id/p/n; the generated column, FK and index were absent.
Defaults were strings `7` and `'x'`; JSON null became an empty CSV/TSV field.
Its JSON row_count was 3 despite having one stored row. Parent returned a then
b with both PK flags true, losing the declared b,a key order. The view returned
two columns, no definition. Two object arguments exited 2 with empty stdout.

Unchecked: live PostgreSQL, MySQL and SQL Server results, permissions/version
variants, cross-catalog runtime behavior, attached-database pool behavior,
large-schema latency and concurrent-DDL consistency. Backend inventory above
is code inspection, not live conformance. No shipping code or original inspect contract was changed.

### Map consequences and notes for the next session

Graduate richer-schema fog into ticket 19 to settle an additive one-call schema
context contract under inspect: object identity plus all columns first, and
explicit scope for relationship/index acquisition. Do not mistake those
acquisitions for a renderer-only task; cut backend work after the contract.
Ticket 16 already owns native DDL fidelity and remains separate. Data row
counts need an explicit cost/estimate policy if selected; do not add COUNT(*)
to every inspect by assumption.

Out-of-scope findings for later work: `cli/inspect.rs:101` splits a name on its
first dot without SQL identifier parsing; :177 matches names case-insensitively
and returns the first schema match rather than reporting ambiguity. The richer
contract must settle safe identity selection, but this ticket fixes neither.
Ticket 03's empty-query-column note concerns adapter query results, not
inspect: inspect constructs its output columns even when metadata rows are
empty (`cli/inspect.rs:79`, :131).
