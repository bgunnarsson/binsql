---
title: "Core builds an estimated plan for one read"
kind: task
mode: afk
status: open
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
