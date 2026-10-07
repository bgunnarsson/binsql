---
title: "Check PostgreSQL behaviour under `--timeout-ms`"
kind: task
mode: afk
status: open
blocked_by: [60, 61, 64]
claimed_by:
---

## Question

Build what 12 settled for this step; the contract is in 12's answer
(`12-statement-budget.md`).

- Live tests:
  - `pg_sleep` in query;
  - a lock wait on a row held by a second session;
  - a timeout during a PostgreSQL `describe` (bound `--arg`);
  - `inspect --catalog <other>`, which counts the per-catalog connect.
- Each exits within N plus 2 s, with no running statement left behind and no row committed.
- Record the results in the ticket, and correct the README if the outcome text overstates anything.

## Answer
