---
title: What fidelity should inspect DDL promise?
kind: grilling
mode: hitl
status: open
blocked_by: [3, 4]
claimed_by:
---

## Question

Choose the objects and fidelity for additive native DDL output: one object
or a selected schema, context for an agent versus a restorable export, ordering,
format and treatment of unsupported objects/backends. Recommend context-only
native definitions first, clearly marking omissions rather than synthesizing
CREATE statements from columns. Keep this under inspect; applying SQL remains
exec. Decide whether calling an external dump utility is acceptable before
assuming pg_dump or a SQL Server scripting dependency. Cut backend acquisition
research/tasks only for the scope selected here.

## Context

Tickets 03 and 04; inspect.rs, core schema.rs and the four adapters.
Primary sources in 03: sqlite3 .schema, pg_dump --schema-only and SQL-native
metadata interfaces. Existing structured inspect formats remain unchanged.

## Answer
