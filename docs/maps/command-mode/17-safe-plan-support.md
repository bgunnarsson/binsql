---
title: Which estimated plans can binsql expose safely?
kind: research
mode: afk
status: resolved
blocked_by: [3]
claimed_by:
---

## Question

Research estimated-plan support for all four backends, native payload formats,
permissions and connection/session-option needs. Distinguish planning from
execution (ANALYZE, SQL Server actual statistics and similar forms). Audit the
existing EXPLAIN classification and inner statement handling, including comments,
options and CTEs, against query and registered-readonly safeguards. Use primary
docs and isolated read-only probes; mark untested backend paths unchecked. Return
an additive interface recommendation and narrow follow-up tasks/research for
safety, without changing SQL code or settling the plan UX here. Runtime-plan
policy and command namespace are subsequent decisions, with 05 consulted.

## Context

Ticket 03; `crates/binsql-core/src/sql.rs:181`, session.rs read-only guards,
`crates/binsql/src/cli/query.rs`, adapters, PostgreSQL EXPLAIN documentation and
backend plan documentation. SQLite EXPLAIN QUERY PLAN already works. No new
drivers or silent write execution under inspect/query.

## Answer

All four backends can give an estimated plan without running the statement, but today's classification lets the executing forms through: `EXPLAIN ANALYZE <write>` is a read to `query` and to a read-only source, and on PostgreSQL and MySQL it runs the write. Fix the classification first (task 22), then add estimated plans only, as a native payload per backend, with the shape settled in 23.

### Backends

| Backend | Estimated, does not execute | Executes | Payload | Needs |
| --- | --- | --- | --- | --- |
| SQLite | `EXPLAIN QUERY PLAN <stmt>`; plain `EXPLAIN` gives bytecode, also without running | none: SQLite has no `EXPLAIN ANALYZE` | rows `id, parent, notused, detail` | nothing |
| PostgreSQL | `EXPLAIN [(FORMAT JSON)] <stmt>` | `EXPLAIN ANALYZE …`, `EXPLAIN (ANALYZE[ TRUE]…) …` run the statement, side effects included | TEXT, JSON, XML or YAML, one row per line or one value | rights to the objects named; nothing else |
| MySQL | `EXPLAIN [FORMAT=TRADITIONAL\|JSON\|TREE] <stmt>`; `DESCRIBE`/`DESC` are synonyms | `EXPLAIN ANALYZE …` runs it, TREE format only | rows, or one JSON/TREE value | rights to the objects named |
| SQL Server | `SET SHOWPLAN_XML ON` as its own batch, the statement, `SET SHOWPLAN_XML OFF` | `SET STATISTICS XML ON` and `SET STATISTICS PROFILE ON` run it | one XML value per statement | `SHOWPLAN` permission; the setting stays on the session until turned off |

Sources: <https://sqlite.org/eqp.html>,
<https://www.postgresql.org/docs/current/sql-explain.html>,
<https://dev.mysql.com/doc/refman/8.4/en/explain.html>,
<https://learn.microsoft.com/en-us/sql/t-sql/statements/set-showplan-xml-transact-sql>.

Probed on SQLite only, against a throwaway database registered read-only
in a project `.binsql.json`, with `binsql query` built from `14dda87`:

- `EXPLAIN QUERY PLAN SELECT * FROM t WHERE id=1` → exit 0, `SEARCH t USING INTEGER PRIMARY KEY (rowid=?)`;
- `/* c */ EXPLAIN QUERY PLAN WITH x AS (SELECT 1) SELECT * FROM x` → exit 0, the comment and CTE are handled;
- `EXPLAIN DELETE FROM t` → exit 0, bytecode, and the row was still there afterwards;
- `EXPLAIN ANALYZE DELETE FROM t` → exit 1, SQLite's syntax error: it reached the database, not the guard;
- `DELETE FROM t` → exit 2, refused by `query`.

PostgreSQL, MySQL and SQL Server paths are unchecked: no server was
reachable, and the table above is from their documentation.

### Classification today

- `classify` (`crates/binsql-core/src/sql.rs:174`) looks only at the first
  word for `EXPLAIN`, `DESCRIBE` and `DESC` (:181) and calls them reads.
  Comments are skipped by the scanner (`words`, :270), so a leading comment
  changes nothing.
- `query` refuses a statement whose `Kind::mutates()` (`sql.rs:36`) without
  `--allow-write` (`crates/binsql/src/cli/query.rs:48`), and
  `Session::guard_read_only` (`crates/binsql-core/src/session.rs:177`)
  refuses a script with one on a read-only source. Both take `classify`'s
  word, so `EXPLAIN ANALYZE DELETE`, `EXPLAIN (ANALYZE) UPDATE` and MySQL's
  `EXPLAIN ANALYZE` over a multi-table `UPDATE`/`DELETE` pass both guards
  and, on PostgreSQL and MySQL, write. `EXPLAIN ANALYZE SELECT f()` runs any
  side effects `f` has, as `SELECT f()` already does: that is the general
  limit of keyword classification, not new to plans.
- `SET SHOWPLAN_XML` and `SET STATISTICS …` are `Kind::Control`, which does
  not mutate, so `query` already accepts them alone. In command mode the
  connection ends with the process, so a setting left on harms nothing; the
  SQL Server adapter sends each statement through `simple_query`
  (`crates/binsql-core/src/adapter/mssql.rs:170`), so the setting is its own
  batch as SQL Server requires.
- No adapter opens a database-level read-only session; the guards above are
  the only protection.

### Recommendation

1. **Task 22**: `EXPLAIN` (and MySQL's `DESCRIBE`/`DESC` with `ANALYZE`)
   takes the kind of the statement it wraps when it executes it: an
   `ANALYZE` keyword or an `ANALYZE` option in a parenthesised list makes the
   whole statement the inner statement's kind, so a wrapped write is a write.
   A plain `EXPLAIN` stays a read whatever it wraps, because it does not run
   it. This tightens a guard that is wrong today; it changes no output and
   no exit except refusing writes that should have been refused.
2. **Estimated plans only.** An agent wants the plan to check an index or a
   join before running something expensive; an executed plan is a run, and
   `exec` already runs statements. binsql builds the backend's estimated form
   itself from a statement it has classified as a single read, so an agent
   never needs to know the per-backend syntax, and returns the backend's
   native payload unchanged (rows for SQLite and traditional MySQL, one JSON
   or XML value otherwise) rather than inventing a common plan format.
3. **Ticket 23** settles the shape: under `inspect` or as a `query` flag
   (05), which payload format per backend, whether a write may be planned
   (the estimated form does not run it, but the guard has to know that), and
   how SQL Server's `SHOWPLAN` permission failure is reported.

Assumed, not asked: runtime plans (`EXPLAIN ANALYZE`, SQL Server actual
statistics) stay out of command mode's plan feature — they execute the
statement, an agent can already run them through `exec`, and leaving them
out is the smaller change and easy to add later.
