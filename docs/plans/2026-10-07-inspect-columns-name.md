---
title: inspect --columns NAME lists one object's columns, matched exactly
date: 2026-10-07
status: in-progress
---

## Context

Ticket `docs/maps/command-mode/26-inspect-columns-exact-name.md` builds the
positional form of `inspect --columns` that ticket 19 settled
(`docs/maps/command-mode/19-schema-context-contract.md`, "### Identity",
:73-84). Ticket 25 shipped `--columns` for every object and refuses a name
(`crates/binsql/src/cli/inspect.rs:23-27`). This plan replaces that refusal.

The contract, settled in 19:

- The name is one exact, case-sensitive object name. It is never split on dots.
  The schema comes only from `--schema`.
- No exact match exits 1 with "no table or view named NAME in CATALOG". If some
  names match when case is ignored, stderr lists them.
- If exact matches sit in more than one schema, the call exits 2. Stderr lists
  the schemas and says to pass `--schema`. It never picks one.
- Plain `inspect NAME` keeps its first-dot split and case-insensitive first
  match (`find`, `inspect.rs:146-172`). It does not change.

The user wants no questions, so routine calls were made here. Each one picks
what changes least:

- The matcher is a pure function, `select`, so it can be unit tested without
  a database. It takes every object in scope and returns every exact match, or
  a `Failure`.
- `select` takes no `schema_given` flag. With `--schema`, `scope` returns one
  schema (`inspect.rs:63-65`), so the matches cannot span two schemas.
  Ambiguity means "exact matches whose `schema` values differ".
- Two exact matches in one schema (a table and a view with the same name)
  cannot happen on the supported backends. If it did, both would be returned
  and listed. They would not count as ambiguous.
- The case-only hint goes into the same error message, after a `;`. It is not a
  separate `note`, because `Failure` carries one message.
  Example: `no table or view named artist in main; names that differ only in
  case: Artist`. Names are printed with `ObjectRef::display()`, byte-sorted and
  deduplicated.
- Ambiguity message: `NAME is in more than one schema (a, b); pass --schema`.
  Schemas are byte-sorted and deduplicated.
- Columns are read only for the selected objects. The output shape, sort, and
  empty-object row and note are the same as with no name (`column_rows`,
  `inspect.rs:229`).

## Relevant lore

- Project note (2026-10-07, mapper): command-mode output is buffered through
  `ResultSet` before it is printed. That is why an error from `select` leaves
  stdout empty: `run` returns before `print`.

## Acceptance criteria

- `binsql inspect --columns artist` on the seeded SQLite fixture prints only
  `artist`'s rows, with the same columns and identity as the no-name form, and
  exits 0.
- On SQLite, a table named `a.b` can be named as `inspect --columns a.b`. The
  output lists its columns, with `object` = `a.b`.
- `inspect --columns ARTIST`, when only `artist` exists, exits 1. Stderr
  contains `no table or view named ARTIST` and lists `artist` as a case-only
  match. Stdout is empty.
- `inspect --columns nonesuch` exits 1 with no case-only hint.
- In a unit test of `select`, exact matches in two schemas return a usage
  `Failure` (exit 2) that names both schemas and `--schema`.
- Plain `inspect ARTIST` still describes `artist`, because matching is
  case-insensitive. Plain `inspect a.b` still splits on the dot, so it does not
  describe the `a.b` table. It exits 1.
- `inspect --columns a b` (two positionals) still exits 2 with "inspect
  describes one table at a time".
- HELP and README document `inspect --columns NAME`. The README Status bullet
  "Naming one table with `inspect --columns`" is gone.

## Tasks

- [ ] **1. Match one object exactly under `--columns`.**
  In `crates/binsql/src/cli/inspect.rs`:
  - Delete the `[_] if every_column` refusal arm (:23-27). `target` then
    carries the name in both modes.
  - Pass `target.as_deref()` into `columns_of` as a new
    `name: Option<&str>` parameter.
  - In `columns_of`, collect every object across `scope` first. If a name was
    given, keep `select(objects, catalog, name)?`. Then read each kept
    object's columns with the existing `session.columns` loop and its
    `columns of …` error.
  - Add `fn select(objects: Vec<ObjectRef>, catalog: &str, name: &str) ->
    Result<Vec<ObjectRef>>`:
    - Keep the objects with `object.name == name`.
    - If none match, return `failed(…)` with the case-only hint when any
      `eq_ignore_ascii_case` names exist.
    - If the matches carry more than one distinct `schema`, return `usage(…)`.
    - Otherwise return the matches.
  - Update the module doc comment (:5-7) to describe the named form.
  - Leave `find` and `describe` untouched.

  Add unit tests in `mod tests`, building objects with the existing
  `object(...)` helper:
  - an exact match is kept and its case variant dropped;
  - no match with a case variant gives `!usage` and a message that contains the
    variant;
  - no match with no variant gives a message with no hint;
  - matches in schemas `a` and `b` give `usage`, and the message names `a`,
    `b` and `--schema`;
  - a name containing a dot matches only the whole name.

  In `crates/binsql/tests/command_mode.rs`, replace
  `inspect_columns_refuses_a_name_until_it_can_match_one_exactly` (:629-638)
  with tests that use `Fixture::new`, `seed` and `direct`:
  - `inspect_columns_names_one_object_exactly`: `--columns artist -o csv`
    gives only `main,,artist,table,…` rows. `--columns ARTIST` is `.failed()`
    and `stderr_has("no table or view named ARTIST")`. It also checks
    `stderr_has("artist")` after the hint text and empty stdout.
    `--columns nonesuch` is `.failed()`.
  - `inspect_columns_names_an_object_with_a_dot`: it runs
    `exec 'CREATE TABLE "a.b" (id INTEGER)'`.
    `inspect --columns a.b -o csv` succeeds with
    `main,,a.b,table,id,INTEGER,…`. Plain `inspect a.b` is `.failed()`, which
    shows that the dot split is kept.
  - Extend `inspect_without_columns_prints_what_it_did_before` (:641) so plain
    `inspect ARTIST -o csv` gives the same stdout as `inspect artist`.

  Verify:
  - `cargo test -p binsql inspect`
  - `cargo test -p binsql --test command_mode inspect`
  - `cargo clippy -p binsql --all-targets -- -D warnings`

- [ ] **2. Document the named form.**
  - In `crates/binsql/src/cli/mod.rs`, add to the INSPECT section of HELP
    (:118-122) that `--columns` takes an optional exact `NAME`, with the schema
    only from `--schema`.
  - In `README.md`:
    - add an example line near :225, such as
      `binsql inspect --columns orders -o json   # one object, exact name`;
    - in the `--columns` paragraph (:321-331), add a few sentences: a name is
      matched exactly and case-sensitively and is never split on dots, so pass
      `--schema` for the schema. No match exits 1 and lists names that differ
      only in case. A name found in several schemas exits 2 until `--schema`
      picks one. Plain `inspect NAME` still matches loosely;
    - remove the Status bullet at :564-565.

  Verify: `cargo test -p binsql` (HELP tests, if any, still pass), then read
  `binsql --help` output and the README section.

## Files

- `crates/binsql/src/cli/inspect.rs`: `run` drops the refusal, `columns_of`
  gains `name` and filters through the new `select`, and new unit tests are
  added. It reuses `scope`, `column_rows`, `failed`/`usage` and
  `ObjectRef::display`.
- `crates/binsql/tests/command_mode.rs`: the refusal test is replaced by the
  exact-name and dotted-name tests, and the unchanged-describe test is
  extended. It reuses `Fixture`, `Run::succeeds/failed/stderr_has`.
- `crates/binsql/src/cli/mod.rs`: HELP INSPECT lines only.
- `README.md`: the inspect example, the `--columns` paragraph and the Status
  bullet.

## Verification

- `cargo test -p binsql`
- `cargo clippy -p binsql --all-targets -- -D warnings`
- Manually, against a scratch SQLite file:
  - `binsql inspect --columns <table> -o json` returns only that table's rows;
  - the wrong-case name exits 1 (`echo $?`) with the hint on stderr and nothing
    on stdout;
  - `binsql inspect <TABLE>` still describes the table.
- Cross-schema ambiguity is covered only by the `select` unit test, because the
  integration fixture is one SQLite file with no schema level. A PostgreSQL
  check (`inspect --columns X` with X in two schemas, which should exit 2) is
  worth doing by hand if a server is available.
