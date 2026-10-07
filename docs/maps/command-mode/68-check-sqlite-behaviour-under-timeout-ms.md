---
title: "Check SQLite behaviour under `--timeout-ms`"
kind: task
mode: afk
status: open
blocked_by: [60, 61, 63]
claimed_by:
---

## Question

Build what 12 settled for this step; the contract is in 12's answer
(`12-statement-budget.md`).

- Tests:
  - a write lock held by a second connection, with a busy wait shorter and one longer than sqlx's default `busy_timeout` (unchecked);
  - a long CPU-bound CTE: check whether the worker stops when the stream is dropped or only at process exit, and that the lock is released after exit.
- Record the results. If the step keeps running, document that the deadline is enforced by the process exiting.

## Answer
