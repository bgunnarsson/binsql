---
title: "PostgreSQL views return their query"
kind: task
mode: afk
status: resolved
blocked_by: [80]
claimed_by:
---

## Question

Build what 16 settled for this step; the contract is in 16's answer
(`16-ddl-output.md`).

- Uses `pg_get_viewdef(oid, true)` for relkind `v` and `m`, giving `Query`. Kinds `r`, `p` and `f` give `Unsupported`.
- Tests: a unit test of the relkind-to-form mapping; a live view and materialized view test only if a PostgreSQL test harness exists (unchecked).
- Updates the README's per-backend row once 84 has added it.

## Answer

A PostgreSQL view or materialized view now gives `query` with the text of
`pg_get_viewdef(oid, true)`; a table, partitioned table or foreign table
gives `unsupported`, as before. The object is looked up by schema and name
with the relkinds `objects()` lists, schema `public` when none is given; no
row is a query error naming the object and schema, and a null view text
gives `withheld`.

Built in docs/plans/2026-10-07-postgres-view-definitions.md
(adapter/postgres.rs, tests/definitions_servers.rs, README.md). Unit tests
cover the relkind-to-form mapping: views and materialized views give their
query, a view without text is withheld, every kind of table is unsupported.
An ignored live test creates a table, a view and a materialized view in
`public` and checks all three and a missing object. Live PostgreSQL, MySQL
and SQL Server unchecked: Docker is not running here.

Neither review found a defect. The correctness review noted that the text
is the server's rebuild, with a leading space and a trailing `;`, and that a
view written as `VALUES` or `WITH` comes back in that form, so the README's
`query` row now says "a view's query" rather than its `SELECT`; and that the
live test leaned on `public` being first in `search_path`, so it now names
`public` everywhere.

Assumed, not asked:
- The lookup uses `nspname` and `relname` with `objects()`'s relkinds, schema `public` when none is given.
- No row is `Error::Query` naming the object and schema.
- A null `pg_get_viewdef` gives `withheld` rather than an empty `query`.
- The relkind-to-form decision is a plain function so it is unit tested without a server.
- The live test lives in a new ignored `tests/definitions_servers.rs`.
