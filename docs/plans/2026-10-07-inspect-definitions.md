---
title: "inspect --definitions prints one definition row per selected object"
date: 2026-10-07
status: active
---

## Context

Ticket [84](../maps/command-mode/84-inspect-definitions-prints-one-definition-row-per.md)
builds the command half of 16's definitions contract on top of 80's
`Session::definition`. Outcome: `inspect --definitions` prints `catalog,
schema, object, kind, form, definition` for every selected table and view,
in every output format.

Settled: unattended loop; work stays on `main`, each task committed and pushed;
no AI mentions in commits.

## Assumed, not asked

- Selection is `columns_of`'s: the objects in scope gathered, then `select`
  for an exact name. Both verbs share one helper for it rather than two copies.
- The stderr note reads `2 definitions not given: 1 unsupported, 1 withheld`,
  naming only the forms that occur, and is left out when every row has text.
- An acquisition failure reads `definition of <object>: <error>`, as
  `--columns` reads `columns of <object>: <error>`.
- `--definitions --columns` is refused before connecting, so it exits 2 even
  with no data source reachable.

## Relevant lore

None in `docs/solutions`. From the map: SQLite fixtures go in the temp dir,
without the `tempfile` crate; SQLite rewrites the opening `CREATE TABLE`
keywords it stores.

## Acceptance criteria

- `inspect --definitions -o json` against a SQLite fixture prints the exact
  envelope and rows; JSONL and CSV carry the same rows.
- `inspect --definitions <name>` gives that object's row only, matched exactly.
- `--definitions --columns` exits 2 with empty stdout.
- The note counting `unsupported` and `withheld` rows is unit tested, and is
  absent when there are none.
- `inspect`, `inspect <t>` and `inspect --columns` print what they did before.
- README documents the switch, the per-backend coverage, the context-not-restore
  caveat and the Status line; HELP lists `--definitions`.

## Tasks

- [x] **1. The switch.** `inspect.rs`: `definitions` switch, refusal with
  `--columns`, `definitions_of`, the note; unit tests; integration tests in
  `tests/command_mode.rs`.
  Verify: `cargo test --workspace`.
- [ ] **2. Docs.** README inspect section, coverage table, caveat, Status;
  HELP in `cli/mod.rs`.
  Verify: read it back.
