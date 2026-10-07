---
title: "Check MySQL behaviour under `--timeout-ms`"
kind: task
mode: afk
status: open
blocked_by: [60, 61, 64]
claimed_by:
---

## Question

Build what 12 settled for this step; the contract is in 12's answer
(`12-statement-budget.md`).

- Live tests: `SLEEP` in query and in exec; an InnoDB lock wait shorter than `innodb_lock_wait_timeout`; DDL inside a timed-out `--tx` batch.
- The message must not suggest the DDL was undone, since MySQL commits it implicitly (`exec.rs:136-154`).
- Record the results.

## Answer
