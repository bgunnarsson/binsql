---
title: "PostgreSQL and MySQL stop the server statement inside a transaction"
kind: task
mode: afk
status: open
blocked_by: []
claimed_by:
---

## Question

Build what 12 settled for this step; the contract is in 12's answer
(`12-statement-budget.md`).

- Give the transaction path the same backend-pid / connection-id capture and `pg_cancel_backend` / `KILL QUERY` that the single-statement path has (`postgres.rs:75-106`, `mysql.rs:57-92`). For example, `run_transaction` takes an optional "on cancel" hook from the adapter.
- Tests: live-server tests on each backend, gated the way the repo gates live tests (unchecked whether such gating exists): `pg_sleep(30)` / `SLEEP(30)` inside `--tx`, cancelled; `pg_stat_activity` / `PROCESSLIST` shows no running statement within the grace.

## Answer
