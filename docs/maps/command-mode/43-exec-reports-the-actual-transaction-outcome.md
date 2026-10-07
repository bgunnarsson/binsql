---
title: "exec reports the actual transaction outcome"
kind: task
mode: afk
status: open
blocked_by: [40]
claimed_by:
---

## Question

Build what 10 settled for this step; the contract is in 10's answer
(`10-structured-errors.md`).

- `run_transaction` (core `session.rs:154`, the adapters) reports whether the rollback was confirmed, whether the commit or rollback failed, and whether the batch was refused before BEGIN. For example, an `Error::Transaction { source, outcome }` whose Display matches today's text.
- `cli/exec.rs:62-65` stops claiming a rollback unless it is `rolled_back`. JSON gets `transaction` (`none` / `rolled_back` / `unknown`) and, for `--no-tx`, `completed` (`:76-82`). Ticket 12's cancellation and timeout reporting should use the same field.
- Tests:
  - a SQLite batch that fails at statement 2 → `rolled_back`, `statement:2`, and the table is unchanged;
  - a read-only source refusing a batch → `transaction:none`, and the text no longer says "rolled back";
  - a two-statement batch with `--no-tx` that fails at statement 2 → `completed:1`;
  - a failed commit is covered by a unit test with a fake adapter if one exists; otherwise it is recorded as unchecked.

## Answer
