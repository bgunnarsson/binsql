---
title: "Core reads object definitions, with SQLite native text"
kind: task
mode: afk
status: resolved
blocked_by: []
claimed_by:
---

## Question

Build what 16 settled for this step; the contract is in 16's answer
(`16-ddl-output.md`).

- Adds `Definition`, `DefinitionForm`, `Adapter::definition` and `Session::definition`.
- SQLite implements it from `<catalog>.sqlite_master.sql`. PostgreSQL, MySQL and SQL Server return `Unsupported` until 81–83 land.
- Tests, against SQLite fixtures:
  - a table and a view, both `Create`, with the exact stored text;
  - an attached second catalog;
  - a table renamed with `ALTER TABLE … RENAME`, checking the stored text is returned untouched;
  - a missing object returns an error, not `Withheld`.
- No flag, no README or HELP change.

## Answer

The core reads an object's own definition text. `Definition` and
`DefinitionForm` live in `schema.rs` and are exported from the crate root;
`Adapter::definition` defaults to `Unsupported` with no text, which is what
PostgreSQL, MySQL and SQL Server return until 81–83; `Session::definition`
hands the call to the object's catalog as `columns` does. SQLite reads
`<catalog>.sqlite_master.sql`, catalog quoted in as `objects()` does and
`main` when none is given: a row gives `Create`, a null `sql` gives
`Withheld`, and no row is `Error::Query` naming the object and catalog.

Built in docs/plans/2026-10-07-core-definitions.md (schema.rs, lib.rs,
adapter/mod.rs, adapter/sqlite.rs, session.rs,
tests/sqlite_definitions.rs). Tests cover a table and a view, an attached
catalog, a table renamed with `ALTER TABLE … RENAME`, and a missing object.
Live PostgreSQL, MySQL and SQL Server unchecked: Docker is not running here.

Two things the tests turned up. SQLite rewrites the opening `CREATE TABLE`
keywords as it stores a table and keeps the rest as typed, so "the text as
stored" is not quite the text as typed; binsql passes the stored text on.
And an `ATTACH` through a session can land on a different pooled connection
from the next call, so the attached-catalog test runs inside the adapter on
a one-connection pool. The correctness and security reviews found nothing.

Assumed, not asked:
- `Adapter::definition` has a default returning `Unsupported`, rather than each backend stating it.
- A missing object is `Error::Query` naming the object and catalog.
- The SQLite lookup matches `type IN ('table', 'view')` as 16 wrote it, not the `ObjectRef`'s kind.
- No catalog on the `ObjectRef` means `main`, as `columns` does.
