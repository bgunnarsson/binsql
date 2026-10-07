---
title: "`binsql query --plan` prints the estimated plan of one read"
date: 2026-10-07
status: done
---

## Context

Ticket 91 (`docs/maps/command-mode/91-query-plan-prints-the-estimated-plan.md`)
builds the command-line half of the estimated-plan feature. The contract is the
"Settled shape" in `23-plan-shape.md`; each backend's estimated form is in
`17-safe-plan-support.md`. The core half landed in ticket 90:
`Session::plan(catalog, sql, limit, cancel)` (`crates/binsql-core/src/session.rs:152`)
plans one plannable read and returns `Error::NotPlannable` for anything else;
`sql::plannable(sql, backend)` (`crates/binsql-core/src/sql.rs:228`) is the
same test, public. The SQL Server SHOWPLAN-permission hint is already in the
core error (commit 98908f9).

Decided (architect, taking the option that changes least):

- `--plan` is a switch in `query.rs`. With it set, after the one-statement
  check and before the mutating check, the command refuses with `usage()`
  (exit 2, nothing sent): `--arg`; a statement that is not
  `sql::plannable`. For a mutating statement the message says to drop
  `--plan`, whether or not `--allow-write` is set. Then it calls
  `session.plan(None, &statement.sql, limit, &cancel)`, maps errors to
  `failed()` (exit 1) and prints through `render::rows` like any query.
- The refusal is checked in the CLI even though `Session::plan` refuses too,
  so the exit code is 2 (usage), not 1, and the message can name the flag.
- Without `--plan` the code path is untouched, so output is byte-identical.
- `--stream` does not exist yet; ticket 72 adds the `--stream`/`--plan` check.
- `--limit` keeps its meaning: it caps collected rows, it does not bound
  server work (see the project note on command-mode `--limit`).

## Relevant lore

None found.

## Acceptance criteria

- `binsql query --plan "SELECT …"` against SQLite exits 0 and prints the
  `EXPLAIN QUERY PLAN` rows (`id, parent, notused, detail`) in `-o json`,
  `-o csv` and `-o raw`.
- A read-only registered source (`prod`) plans a read.
- Exit 2, with nothing sent, for: `--plan` over `EXPLAIN …` and `PRAGMA …`;
  `--plan` with `--arg`; `--plan` over a `DELETE` with `--allow-write` — the
  message says to drop `--plan`, and the row is still in the table afterwards.
- `query` without `--plan` behaves exactly as before (existing tests pass
  unchanged).
- `binsql help` QUERY section and the README query section document `--plan`:
  the per-backend table, `-o raw` for the bare document, reads only, the
  SQL Server `GRANT SHOWPLAN TO <user>` requirement, estimate only (the
  statement is not run).

## Tasks

- [x] Add the `--plan` switch to `query` and its tests. In `query.rs` add
  `"plan"` to `SWITCHES`; after the `let [statement] = …` check, branch on
  `args.is_set(&["plan"])`: refuse `args.value(&["arg"]).is_some()` (or the
  same test `bind_values` uses for presence — non-empty `params`) with
  `usage("--plan takes no --arg …")`; refuse `!sql::plannable(&statement.sql, backend)`
  with `usage`, wording for `statement.kind.mutates()` as "--plan plans reads
  only; drop --plan to run a {label} statement" plus the `sql::summarize`
  line, otherwise "--plan plans one SELECT, WITH, VALUES or TABLE statement";
  then `cancel_on_interrupt()`, `session.plan(None, &statement.sql, limit, &cancel)`
  mapped to `failed`, and `print(&render::rows(..))`. In
  `crates/binsql/tests/command_mode.rs`, following the query tests at
  `:171-244` (`Fixture::new/seed/direct/write_config`, `Run::succeeds/refused/failed/stdout_has/stderr_has`):
  json, csv and raw output of a SQLite plan (assert on `detail` text such as
  `SCAN`); the `prod` read-only source plans a read; exit 2 for `EXPLAIN`,
  `PRAGMA`, `--arg`, and `DELETE` with `--allow-write` followed by a query
  showing the row remains. Verify: `cargo test -p binsql --test command_mode`
  and `cargo clippy --workspace --all-targets`.
- [x] Document `--plan`. Add a `--plan` line to the QUERY part of `HELP`
  (`crates/binsql/src/cli/mod.rs:105-109`) and to the README query flags
  (`README.md:265-269`), with a short subsection: the backend table from 23
  (SQLite `EXPLAIN QUERY PLAN` rows; PostgreSQL `EXPLAIN (FORMAT JSON)` one
  JSON value; MySQL `EXPLAIN FORMAT=JSON` one text value; SQL Server
  `SHOWPLAN_XML` one XML value), `-o raw` to get the bare document for jq or an
  XML tool, reads only, SQL Server needs `GRANT SHOWPLAN TO <user>`, and that
  it is an estimate — the statement is never run. Verify: `cargo test -p binsql
  --test command_mode` (help output tests, if any, still pass) and
  `cargo clippy --workspace --all-targets`.

## Files

- `crates/binsql/src/cli/query.rs`: `"plan"` in `SWITCHES`; the `--plan`
  branch, reusing `usage`, `failed`, `sql::plannable`, `sql::summarize`,
  `statement.kind.mutates()/label()`, `cancel_on_interrupt`, `render::rows`.
- `crates/binsql/tests/command_mode.rs`: the new tests, reusing `Fixture` and `Run`.
- `crates/binsql/src/cli/mod.rs`: the `--plan` line in the HELP QUERY section.
- `README.md`: `--plan` in the query flags and its subsection.

## Verification

`cargo test -p binsql --test command_mode` and
`cargo clippy --workspace --all-targets` both clean. By hand against a SQLite
file: `binsql query --plan -o raw "SELECT * FROM t WHERE id = 1"` prints the
plan rows; `binsql query --plan --allow-write "DELETE FROM t"` exits 2 telling
you to drop `--plan`, and `SELECT count(*) FROM t` is unchanged;
`binsql help` shows the `--plan` line. Live PostgreSQL, MySQL and SQL Server
checks are tickets 92 and 93.
