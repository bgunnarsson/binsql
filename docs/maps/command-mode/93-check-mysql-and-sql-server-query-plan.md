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
