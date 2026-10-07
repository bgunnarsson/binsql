---
title: The core builds an estimated plan for one plannable read, on every backend
date: 2026-10-07
status: in-progress
---

## Context

Ticket 90 (`docs/maps/command-mode/90-core-builds-an-estimated-plan-for-one-read.md`)
builds the core half of `binsql query --plan`. The contract is the "Settled
shape" in the Answer of `docs/maps/command-mode/23-plan-shape.md`: estimated
plans only, for one statement that classifies `Kind::Read` and starts with
`SELECT`, `WITH`, `VALUES` or `TABLE`. Each backend's own result set comes back
unchanged:

| Backend | Sent | Returns |
| --- | --- | --- |
| SQLite | `EXPLAIN QUERY PLAN <stmt>` | rows `id, parent, notused, detail` |
| PostgreSQL | `EXPLAIN (FORMAT JSON) <stmt>` | one row, column `QUERY PLAN` |
| MySQL | `EXPLAIN FORMAT=JSON <stmt>` | one row, column `EXPLAIN` |
| SQL Server | `SET SHOWPLAN_XML ON`, the statement, `SET SHOWPLAN_XML OFF`, each its own `simple_query` batch under one lock, OFF sent even after an error | one XML value |

Ticket 91 (the CLI `--plan` switch, its exit-2 refusals, README and HELP) is
separate and not in this plan. Live checks against PostgreSQL, MySQL and SQL
Server are tickets 92 and 93. The user wants no questions, so the routine calls
below were made by the architect, picking what changes least:

- **Refusal.** `Session::plan` refuses with a new variant,
  `Error::NotPlannable { statement }`, displayed as
  `only one SELECT, WITH, VALUES or TABLE statement can be planned; refusing to plan {statement}`.
  This is a refusal, not a failure: nothing is sent, as with `Error::ReadOnly`
  (`crates/binsql-core/src/error.rs:21-27`). Command mode maps every core error
  to exit 1 (`crates/binsql/src/cli/query.rs:61-63`). Ticket 91 calls
  `sql::plannable` itself first, to exit 2. No `match` on `Error` is exhaustive:
  the only one, `crates/binsql/src/app/mod.rs:943-977`, has an `Err(error)`
  catch-all. So the new variant needs no arm anywhere.
- **One statement.** `Session::plan` takes `&str`, not `&Bound`: `--arg` with
  `--plan` is refused, so there is never a value to bind. It runs `sql::split`
  and refuses anything other than exactly one plannable statement.
  `plannable` reads only the first word, so `SELECT 1; DELETE FROM t` would pass
  it. Prefixing `EXPLAIN QUERY PLAN` to that script would then plan the select
  and run the delete. The single-statement check is what stops it, and it
  belongs in the core, not only in the CLI's check at `query.rs:38-46`.
- **`plan_sql` is total.** `sql::plan_sql(sql, backend) -> String` returns the
  statement unchanged for `Backend::MsSql`, because SQL Server brackets it with
  session settings rather than prefixing it. The default `Adapter::plan` and the
  SQL Server override can then both call it, and there is no `Option` to unwrap.
- **SHOWPLAN hint.** On SQL Server, an `Error::Query` whose message contains
  `SHOWPLAN` (ASCII case-insensitive) is rebuilt with the message
  `{server message}\n  estimated plans on SQL Server need the SHOWPLAN permission (GRANT SHOWPLAN TO <user>)`.
  The indented second line is the CLI's existing continuation idiom
  (`crates/binsql/src/cli/exec.rs:63`). The CLI prints `error: {message}`
  (`crates/binsql/src/cli/mod.rs:72`), which gives the two lines 23 settled. The
  native code 262 waits for ticket 42.
- **OFF that fails.** If `SET SHOWPLAN_XML OFF` itself fails, the connection is
  reopened before the lock is released. Otherwise the next statement on the
  connection would return its plan instead of running. A cancel reopens the
  connection already (`mssql.rs:109-112`), and a new connection does not have
  SHOWPLAN on, so no OFF is needed after a cancel.

## Relevant lore

- Project note (2026-10-07, lead, `crates/binsql-core/src/sql.rs`): `scan` ends
  a block comment at the first `*/`, and it treats MySQL `/*! … */` as inert
  although MySQL runs it. `plannable` inherits both blind spots through
  `classify`. Here they are bounded: the planned form never runs the statement
  on SQLite, PostgreSQL or MySQL (plain `EXPLAIN` without `ANALYZE`), and on SQL
  Server only with SHOWPLAN on. So this plan relies on the single-statement
  check, not on comments being inert.
- Project note (2026-10-07, mapper): `--limit` caps collected rows client-side
  in `sqlx_common.rs` and `mssql.rs`. `plan` passes `limit` through to the same
  paths and adds no server-side limiting.

## Acceptance criteria

- `sql::plannable("SELECT 1", b)` and `plannable("WITH x AS (SELECT 1) SELECT * FROM x", b)`
  are true. A writing CTE, `EXPLAIN SELECT 1`, `SHOW TABLES`, `PRAGMA table_info(t)`
  and `INSERT …` are false.
- `sql::plan_sql` gives `EXPLAIN QUERY PLAN <stmt>` for SQLite,
  `EXPLAIN (FORMAT JSON) <stmt>` for PostgreSQL, `EXPLAIN FORMAT=JSON <stmt>` for
  MySQL and `<stmt>` unchanged for SQL Server.
- `Adapter::plan` exists with a default that sends `plan_sql` through `run`.
  `MsSqlAdapter` overrides it with ON / statement / OFF under one lock, OFF sent
  after an error. A result that is not one row of one column is an
  `Error::Query` reading `SQL Server returned no plan`. An error naming SHOWPLAN
  carries the GRANT hint line.
- On SQLite, `Session::plan(None, "SELECT * FROM widget WHERE id = 1", None, &token)`
  returns columns `id, parent, notused, detail`, and the table's rows are
  unchanged afterwards. `Session::plan` on `INSERT …`, on `SHOW …`, or on two
  statements returns `Error::NotPlannable`, and nothing reaches the server.
- On a read-only source, `Session::plan` of a read succeeds.
- `cargo test --workspace` and `cargo clippy --workspace --all-targets` pass.

## Tasks

- [x] **`sql::plannable` and `sql::plan_sql`.** In `crates/binsql-core/src/sql.rs`,
  below `classify` (:175):
  `pub fn plannable(sql, backend) -> bool` is
  `classify(sql, backend) == Kind::Read` and `words(sql, backend, 1)` has a first
  word in `SELECT | WITH | VALUES | TABLE`. `classify` already reads `(SELECT 1)`
  as `SELECT` (`adapter/mod.rs:93-94`) and finds the writing CTE (:186-196).
  `pub fn plan_sql(sql, backend) -> String` uses a `match backend` that prefixes
  per the table and returns `sql.to_string()` for `MsSql`. Tests go in
  `mod tests` (:628): plannable on `SELECT`, `WITH … SELECT`,
  `WITH d AS (DELETE FROM t RETURNING *) SELECT * FROM d`, `EXPLAIN SELECT 1`,
  `SHOW TABLES`, `PRAGMA table_info(t)`, `INSERT INTO t VALUES (1)`, and
  `/* c */ SELECT 1`; plan_sql for all four backends.
  Verify: `cargo test -p binsql-core sql::tests`, then
  `cargo clippy --workspace --all-targets`. Commit.
- [x] **`Adapter::plan` default, `Error::NotPlannable`, `Session::plan`, and the
  SQLite round trip.** In `adapter/mod.rs`, after `run` (:48-53), add a default
  method. `#[async_trait]` (:19) allows default async bodies:
  `async fn plan(&self, statement: &Bound, limit, cancel) -> Result<ResultSet>`
  builds `Bound { sql: sql::plan_sql(&statement.sql, self.backend()), params: statement.params.clone() }`
  and returns `self.run(&planned, limit, cancel).await`. Its doc comment says the
  statement is planned, not run, and that SQL Server overrides it. In
  `error.rs`, add `NotPlannable { statement: String }` beside `ReadOnly`, with a
  doc comment saying it is a refusal. In `session.rs`, after `run_bound`
  (:134-146), add
  `pub async fn plan(&self, catalog: Option<&str>, sql: &str, limit, cancel) -> Result<ResultSet>`.
  It splits with `sql::split(sql, backend)`. Unless the split gives exactly one
  statement and `sql::plannable(&it.sql, backend)` holds, it returns
  `Error::NotPlannable { statement: sql::summarize(sql, backend, 80) }`. It then
  calls `self.guard_read_only(&it.sql)?`, and
  `self.adapter_for(catalog).await?.plan(&Bound::plain(it.sql), limit, cancel).await`.
  In `tests/sqlite_roundtrip.rs`, add a `#[tokio::test]` that uses
  `temp_database` (:9) and `source` (:18). It creates and fills `widget`, plans
  `SELECT * FROM widget WHERE id = 1`, and asserts the column names are
  `["id", "parent", "notused", "detail"]` and that there is at least one row. It
  asserts `SELECT count(*) FROM widget` is unchanged, and that
  `plan(None, "DELETE FROM widget", …)`, `plan(None, "PRAGMA table_info(widget)", …)`
  and `plan(None, "SELECT 1; DELETE FROM widget", …)` each give
  `Error::NotPlannable`. It reopens the same file with `source(&path, true)` and
  asserts a plan of the select succeeds.
  Verify: `cargo test -p binsql-core --test sqlite_roundtrip`,
  `cargo test --workspace`, `cargo clippy --workspace --all-targets`. Commit.
- [ ] **SQL Server override.** In `adapter/mssql.rs`, add
  `const SHOWPLAN_HINT: &str = "estimated plans on SQL Server need the SHOWPLAN permission (GRANT SHOWPLAN TO <user>)";`
  and a pure `fn showplan_hint(error: Error) -> Error`. For an `Error::Query`
  whose `to_string().to_ascii_uppercase()` contains `SHOWPLAN`, it returns
  `Error::query(anyhow!("{error}\n  {SHOWPLAN_HINT}"))`. Every other error comes
  back unchanged. In `impl Adapter for MsSqlAdapter`, after `run` (:418), add
  `plan`, documented like `run_transaction` (:428-430). If
  `statement.params` is non-empty it returns
  `Error::query(anyhow!("SQL Server plans take no bound values"))`. Otherwise it
  takes `let mut client = self.client.lock().await;` and uses `collect` (:158)
  with `&[]` params, which is what makes each batch its own `simple_query`
  (:169-170). (`run_one` cannot be used for the SETs: it routes `SET` to
  `execute_one`, which calls `client.execute`.) The steps:
  1. `collect(&mut client, "SET SHOWPLAN_XML ON", &[], None, cancel)`. On
     `Err(e)`, return `Err(showplan_hint(e))`. On `Ok(None)`, return
     `self.cancelled(&mut client)`.
  2. `let planned = collect(&mut client, &sql::plan_sql(&statement.sql, Backend::MsSql), &[], limit, cancel).await;`.
     On `Ok(None)`, return `self.cancelled(&mut client)`: the new connection
     has SHOWPLAN off.
  3. Send `collect(&mut client, "SET SHOWPLAN_XML OFF", &[], None, &CancellationToken::new())`.
     If that fails, `*client = open(&self.config, &self.label).await?`. Then,
     if `planned` was an error, return `showplan_hint(error)`. If OFF failed
     and the plan did not, return the OFF error.
  4. If the result's `columns.len() != 1 || rows.len() != 1`, return
     `Error::query(anyhow!("SQL Server returned no plan"))`. Otherwise return
     the result.
  Unit tests, in a `#[cfg(test)] mod tests` in `mssql.rs` (or the existing one
  there, if it has one): `showplan_hint` on
  `Error::query(anyhow!("SHOWPLAN permission denied in database 'x'."))` ends with
  `SHOWPLAN_HINT` and keeps the server message first; on a lowercase `showplan`
  message it also adds the hint; on `Error::query(anyhow!("Invalid object name 't'."))`
  and on `Error::Cancelled` it leaves the message unchanged.
  Verify: `cargo test -p binsql-core showplan`, `cargo test --workspace`,
  `cargo clippy --workspace --all-targets`. Commit, and resolve ticket 90's
  Answer with what was built.

## Files

- `crates/binsql-core/src/sql.rs`: `plannable` and `plan_sql` beside
  `classify` (:175), reusing `classify` and `words` (:329). Unit tests in
  `mod tests` (:628).
- `crates/binsql-core/src/error.rs`: the `NotPlannable { statement }` variant,
  shaped like `ReadOnly` (:21-27).
- `crates/binsql-core/src/adapter/mod.rs`: the default `Adapter::plan` after
  `run` (:48-53), built on `run` and `sql::plan_sql`.
- `crates/binsql-core/src/session.rs`: `Session::plan` after `run_bound`
  (:134-146), reusing `sql::split`, `sql::summarize`, `guard_read_only`
  (:178) and `adapter_for` (:76).
- `crates/binsql-core/src/adapter/mssql.rs`: the `plan` override, reusing
  `collect` (:158), `cancelled` (:109), `open` (:249) and the
  closing-statement-after-error pattern of `run_transaction` (:431-478). Also
  `SHOWPLAN_HINT`, `showplan_hint`, and their unit tests.
- `crates/binsql-core/tests/sqlite_roundtrip.rs`: one plan round trip test,
  reusing `temp_database` (:9) and `source` (:18).

## Verification

- `cargo test --workspace`: all existing tests, the new `sql::tests` cases, the
  `showplan_hint` tests and the SQLite plan round trip pass.
- `cargo clippy --workspace --all-targets` is clean.
- Read the SQLite test's assertions: the plan columns are exactly `id, parent,
  notused, detail`, the row count of `widget` is unchanged, and the three
  refusals are `Error::NotPlannable`.
- SQL Server cannot run here. Its override is proven only by compiling and by
  the `showplan_hint` unit tests. The real ON / statement / OFF exchange, the
  shape of the XML column, and the permission error's wording are left to the
  live checks 92 and 93. Until those run, the column type that PostgreSQL and
  MySQL give the JSON plan is unchecked too.
