---
title: "PostgreSQL views return their query"
kind: task
mode: afk
status: open
blocked_by: [80]
claimed_by:
---

## Question

Build what 16 settled for this step; the contract is in 16's answer
(`16-ddl-output.md`).

- Uses `pg_get_viewdef(oid, true)` for relkind `v` and `m`, giving `Query`. Kinds `r`, `p` and `f` give `Unsupported`.
- Tests: a unit test of the relkind-to-form mapping; a live view and materialized view test only if a PostgreSQL test harness exists (unchecked).
- Updates the README's per-backend row once 84 has added it.

## Answer
