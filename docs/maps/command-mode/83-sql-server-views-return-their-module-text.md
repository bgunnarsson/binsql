---
title: "SQL Server views return their module text"
kind: task
mode: afk
status: resolved
blocked_by: [80]
claimed_by:
---

## Question

Build what 16 settled for this step; the contract is in 16's answer
(`16-ddl-output.md`).

- Uses `sys.sql_modules.definition` for views: present gives `Create`, null gives `Withheld`. Tables give `Unsupported`.
- Tests: a unit test of the null-to-`Withheld` mapping and of the kind mapping.
- Updates the README's per-backend row once 84 has added it.

## Answer

A SQL Server view now gives `create` with its `sys.sql_modules.definition`,
the module as it was written; a view whose text is null (created `WITH
ENCRYPTION`, or no VIEW DEFINITION permission) gives `withheld`, and a table
gives `unsupported`. One query in the object's catalog reads the type from
`sys.objects` and the text through a left join on `sys.sql_modules`; no row
is a query error naming the object and schema.

Built in docs/plans/2026-10-07-sql-server-view-definitions.md
(adapter/mssql.rs, tests/definitions_servers.rs, README.md). Unit tests
cover the type-to-form mapping and the Unicode literal. An ignored live test
creates a table and a view in `dbo` and checks both and a missing object.
Live PostgreSQL, MySQL and SQL Server unchecked: Docker is not running here.

The correctness review found that `quote_literal` wrote `'...'` without `N`,
so a schema or name outside the database's code page became `?`s and
matched nothing, failing `definition` and the existing `columns` alike; it
now writes `N'...'`. The security review found no injection and noted that
a missing VIEW DEFINITION permission also gives `withheld`; the README now
says so.

Assumed, not asked:
- The lookup uses `sys.schemas.name` and `sys.objects.name` with the types `objects()` lists, in the object's catalog through `qualify`, schema `dbo` when none is given.
- No row is `Error::Query` naming the object and schema; an object the login cannot see reads the same as a missing one.
- The type-to-form decision is a plain function so it is unit tested without a server.
- The text is the module as written: leading comments, `CREATE OR ALTER` and a name from before `sp_rename` stay. The README says so.
- The live test is added to the ignored `tests/definitions_servers.rs` and reads `BINSQL_TEST_MSSQL`.

Live, 2026-10-07, on `eimskip/local` (Azure SQL 12.0.2000.8, read-only source):

- `inspect --definitions -o csv` exits 0 in about 5 s. It prints the header and 103 table rows, each `form` `unsupported` with an empty `definition`, and stderr reads `103 definitions not given: 103 unsupported`.
- `inspect --definitions -o json __EFMigrationsHistory` gives one row with `definition: null`.
- A system view (`sys.database_firewall_rules`) is not selected: "no table or view named …".

Neither `eimskip/local` nor `osar/local` has a user view, so a view's `create` text and a view with null text (`WITH ENCRYPTION`) remain unchecked live.
