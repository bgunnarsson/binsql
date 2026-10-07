---
title: "Core builds an estimated plan for one read"
kind: task
mode: afk
status: resolved
blocked_by: []
claimed_by:
---

## Question

Build what 23 settled for this step (see `23-plan-shape.md`, "Settled shape"):

- `sql::plannable(sql, backend) -> bool`: true for a `Kind::Read` whose first word is `SELECT`, `WITH`, `VALUES` or `TABLE`.
- A pure `plan_sql(sql, backend)` builds the estimated form: SQLite `EXPLAIN QUERY PLAN <stmt>`, PostgreSQL `EXPLAIN (FORMAT JSON) <stmt>`, MySQL `EXPLAIN FORMAT=JSON <stmt>`.
- `Adapter::plan(statement, limit, cancel) -> Result<ResultSet>`; the default sends `plan_sql` through `run`.
- `MsSqlAdapter` overrides it: under one lock, `simple_query` for `SET SHOWPLAN_XML ON`, the statement, and `SET SHOWPLAN_XML OFF`, sending OFF even after an error. A result that is not one row with one column exits 1 with "SQL Server returned no plan". An error whose message names SHOWPLAN gets the line `estimated plans on SQL Server need the SHOWPLAN permission (GRANT SHOWPLAN TO <user>)`.
- `Session::plan` refuses a statement that is not plannable and applies `guard_read_only`.
- Tests:
  - `plannable`: SELECT, WITH, a writing CTE, EXPLAIN, SHOW, PRAGMA, INSERT;
  - `plan_sql` for each backend;
  - SQLite `Session::plan` returns `id, parent, notused, detail` and leaves the data unchanged;
  - all existing tests pass.

## Context

- 23's answer is the contract; 17's answer has each backend's estimated form and sources.

## Answer

The core plans one read without running it: Session::plan, sql::plannable and sql::plan_sql, on every backend (plan docs/plans/2026-10-07-core-estimated-plan.md).

Built:
- `sql::plannable(sql, backend)` is true for a `Kind::Read` whose first word is SELECT, WITH, VALUES or TABLE. `sql::plan_sql` gives `EXPLAIN QUERY PLAN` (SQLite), `EXPLAIN (FORMAT JSON)` (PostgreSQL), `EXPLAIN FORMAT=JSON` (MySQL), and the statement unchanged for SQL Server.
- `Session::plan(catalog, sql, limit, cancel)` refuses anything but exactly one plannable statement with `Error::NotPlannable` (a refusal; nothing is sent), then applies the read-only guard. A read-only source can still plan.
- `Adapter::plan` defaults to running the prefixed statement (SQLite). PostgreSQL and MySQL override it to send the prefixed statement prepared, so the server refuses a second statement that the lexer missed (nested PostgreSQL comments, `E''` strings, MySQL `--x`). Review found those let `plan` run a write through `raw_sql`.
- SQL Server sends `SET SHOWPLAN_XML ON`, the statement and `OFF` as separate batches under one lock, with OFF sent after an error. A `showplan` flag on the adapter makes the next statement replace the connection when a plan stopped before OFF (cancelled, dropped, or OFF refused). A result other than 1×1 is "SQL Server returned no plan". An error naming SHOWPLAN gains the GRANT hint line.

Assumed, not asked:
- Refusal is a new `Error::NotPlannable` (exit 1 through the CLI's core-error path). Ticket 91 checks `sql::plannable` itself first to exit 2.
- PostgreSQL and MySQL plans go prepared. The MySQL docs do not list EXPLAIN among preparable statements; MySQL parses EXPLAIN SELECT as a SELECT, so it should prepare, but if it does not, plans fail with a server error rather than run anything. Live check 93 settles it.

Not checked live: PostgreSQL, MySQL, SQL Server (tickets 92, 93).
