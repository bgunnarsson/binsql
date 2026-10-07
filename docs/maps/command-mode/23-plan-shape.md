---
title: What shape does an estimated plan take on the command line?
kind: grilling
mode: hitl
status: resolved
blocked_by: [17, 22]
claimed_by:
---

## Question

17 recommends estimated plans only, built by binsql from a single classified
statement, with each backend's native payload returned unchanged. Settle:

- where it lives: an `inspect` form or a `query` flag (05 rules out a new
  verb);
- which payload each backend returns (SQLite rows, PostgreSQL JSON or text,
  MySQL JSON, TREE or traditional rows, SQL Server showplan XML) and how it
  maps onto `--format`;
- whether a write may be planned, given its estimated form does not run it
  and `query` refuses writes without `--allow-write`;
- how a missing SQL Server `SHOWPLAN` permission is reported;
- then cut the build ticket.

## Context

- 17's answer and links; 05's namespace decision; 03's survey.

## Answer

`binsql query --plan` returns the backend's estimated plan for one plannable read (SELECT, WITH, VALUES, TABLE), built by binsql and returned as the backend's own result set through `-o`; writes, `--arg` and `--stream` are refused with exit 2, and nothing new runs the statement.

### Settled shape

- **Where:** a switch on `query`, not an `inspect` form. `query` already takes the SQL from an argument, `-f` or stdin (`crates/binsql/src/cli/mod.rs:257`) and checks for one statement (`crates/binsql/src/cli/query.rs:38-46`); `inspect` takes an object name (`inspect.rs:18-22`). 05 allows a read-only query flag. Assumed, not asked: the switch is named `--plan` — short, says what it returns, easy to remove.
- **Accepts:** one statement that classifies `Kind::Read` and starts with `SELECT`, `WITH`, `VALUES` or `TABLE` (a new `sql::plannable`). `EXPLAIN`, `DESCRIBE`, `SHOW` and `PRAGMA` are reads but cannot be planned again: exit 2. A mutating statement exits 2 even with `--allow-write`, and the message says to drop `--plan`. Assumed, not asked: reads only — SQL Server is safe only if `SHOWPLAN_XML` really took effect, a read run by mistake harms nothing and a write would; allowing writes later is additive.
- **Sent and returned:**

| Backend | Sent | Returns |
| --- | --- | --- |
| SQLite | `EXPLAIN QUERY PLAN <stmt>` | rows `id, parent, notused, detail` |
| PostgreSQL | `EXPLAIN (FORMAT JSON) <stmt>` | one row, column `QUERY PLAN`, a JSON value |
| MySQL | `EXPLAIN FORMAT=JSON <stmt>` | one row, column `EXPLAIN`, text (column type unchecked) |
| SQL Server | `SET SHOWPLAN_XML ON`, the statement, `SET SHOWPLAN_XML OFF`, each its own `simple_query` batch under one lock, OFF sent even after an error | one XML value as text |

  Assumed, not asked: JSON for PostgreSQL and MySQL and no `--plan-format` — JSON is what an agent parses, and a format option can come later. Assumed, not asked: MySQL's JSON text is not re-typed as JSON; it stays the native payload.
- **`-o`:** the result goes through `render::rows` (`crates/binsql/src/render.rs:70`) unchanged, so every format works; `-o raw` prints the bare document, which the README recommends for jq or XML tools.
- **SHOWPLAN denied:** exit 1, `error: <server message>`, then `estimated plans on SQL Server need the SHOWPLAN permission (GRANT SHOWPLAN TO <user>)` when the message names SHOWPLAN (and, once 42 lands, when the native code is 262).
- **Other flags:** `--allow-write` changes nothing; a read-only source may plan (the guard sees a read); `--arg` with `--plan` exits 2 (Assumed, not asked: parameters in EXPLAIN and showplan are unchecked on the servers, and adding them later is additive); `--stream` with `--plan` exits 2, added by whichever of 72 and 91 lands second; `--limit`, ⌃C and `--timeout-ms` keep their meaning.
- **Docs:** `--plan` in the QUERY part of `HELP` and the README query section.

Unchecked: that tiberius's `execute` sends `sp_executesql` (the plan path uses `simple_query` regardless), and the column type MySQL gives `EXPLAIN FORMAT=JSON`. Build tickets: 90 and 91; live checks 92 and 93.
