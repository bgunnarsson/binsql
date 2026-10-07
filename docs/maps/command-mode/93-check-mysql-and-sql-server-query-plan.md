---
title: "Check MySQL and SQL Server `query --plan`"
kind: task
mode: afk
status: open
blocked_by: [91]
claimed_by:
---

## Question

Build what 23 settled for this step (see `23-plan-shape.md`, "Settled shape"):

A live check against MySQL and SQL Server. Confirm:

- the type MySQL gives `EXPLAIN FORMAT=JSON`'s column, and what `-o json` prints for it;
- SQL Server returns one XML value, and the session's SHOWPLAN is off afterwards;
- a SQL Server login without SHOWPLAN gets exit 1, the server's message and the hint line;
- whether tiberius's `execute` sends `sp_executesql` (the plan path uses `simple_query` either way).

Mark anything not run as unchecked.

## Context

- 23's answer is the contract; 17's answer has each backend's estimated form and sources.

## Answer

Partial, 2026-10-07. Still open: MySQL has no configured source.

SQL Server, checked on `eimskip/local` (Azure SQL 12.0.2000.8, read-only source):

- `query --plan "SELECT TOP 1 1 AS x FROM sys.objects"` returns one row with one nvarchar column, `Microsoft SQL Server 2005 XML Showplan`, holding the `<ShowPlanXML …>` document. It exits 0 in about 40 ms. With `-o json` the value prints as a single JSON string.
- The login holds SHOWPLAN (`HAS_PERMS_BY_NAME(NULL,'DATABASE','SHOWPLAN')` = 1).

Unchecked:

- That SHOWPLAN is off afterwards. A one-shot CLI run does not reuse the session, so checking needs a test that holds the connection open.
- A login without SHOWPLAN. Checking needs a login created for the purpose.
- Whether tiberius's `execute` sends `sp_executesql`. Checking needs a server-side trace.
- All of MySQL.
