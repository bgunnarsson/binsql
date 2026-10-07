---
title: Which estimated plans can binsql expose safely?
kind: research
mode: afk
status: claimed
blocked_by: [3]
claimed_by: lead
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
