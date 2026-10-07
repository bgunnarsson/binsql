---
title: What does an opt-in statement deadline bound?
kind: grilling
mode: hitl
status: open
blocked_by: [3]
claimed_by:
---

## Question

Choose an additive statement deadline for query, exec and inspect reads.
Recommend milliseconds with no deadline added by default. Settle whether a
batch receives a budget per statement or for the whole batch, whether bind
preparation and metadata reads count, and what happens during cancellation,
rollback and reconnect. Keep cleanup bounded and report unknown write outcome
honestly; a timeout is not proof of rollback or permission to retry. Distinguish
execution deadlines from lock/busy timeouts and connect budgets. Cut tasks small
enough to validate adapter-specific behaviour across the four backends.

## Context

Ticket 03, PostgreSQL and sqlcmd timeout sources it cites;
`crates/binsql/src/cli/query.rs`, exec.rs, inspect.rs;
`crates/binsql-core/src/session.rs`, adapter/sqlx_common.rs and adapter/mssql.rs.
Read 10/11 if resolved; preserve their error/connect contracts.

## Answer
