---
title: "Core reads object definitions, with SQLite native text"
date: 2026-10-07
status: active
---

## Context

Ticket [80](../maps/command-mode/80-core-reads-object-definitions-with-sqlite-native-text.md)
builds the core half of 16's definitions contract. Outcome: `Definition`,
`DefinitionForm`, `Adapter::definition` and `Session::definition`; SQLite
returns the text `sqlite_master` stores, and the other backends return
`Unsupported` until 81–83 land.

Settled: unattended loop; work stays on `main`, each task committed and pushed;
no AI mentions in commits. Additions only: no flag, no README or HELP change.

## Assumed, not asked

- `Adapter::definition` has a default returning `Unsupported` with no text,
  which is what PostgreSQL, MySQL and SQL Server need until 81–83 override it.
- A missing object is `Error::Query` naming the object and catalog, so 84 can
  report it as a database failure that names the object.
- The SQLite lookup matches `type IN ('table', 'view')` as 16 wrote it, not
  the `ObjectRef`'s kind, so a caller that has the kind wrong still gets the
  object's own text.
- No catalog on the `ObjectRef` means `main`, as `columns` does.

## Relevant lore

None in `docs/solutions`. From the map: SQLite test files go in the temp dir,
without the `tempfile` crate. `ATTACH` holds for one pooled connection only;
a session used one call at a time never opens a second.

## Acceptance criteria

- A table and a view each give `Create` with the text SQLite stored: the body
  exactly as typed, whitespace, case and comments included. SQLite itself
  rewrites the opening `CREATE TABLE` keywords, and that is passed on as is.
- An object in an attached catalog gives its own text, not `main`'s. Tested
  inside the adapter on a one-connection pool, since a session's `ATTACH`
  can land on a different pooled connection from the next call.
- After `ALTER TABLE … RENAME`, the text is what SQLite stored, untouched.
- A missing object is an error, not `Withheld`.
- All existing tests pass unchanged; clippy is clean.

## Tasks

- [x] **1. Definitions in the core.** `schema.rs` types, exported from the
  crate root; `Adapter::definition` with its default; SQLite's override;
  `Session::definition`; `tests/sqlite_definitions.rs`.
  Verify: `cargo test --workspace`.
