---
title: "exec reports the actual transaction outcome"
kind: task
mode: afk
status: resolved
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

A failed transactional `exec` batch says how it ended — `transaction` is `rolled_back` when the rollback was confirmed, `unknown` when the rollback or commit failed or the batch may have committed part of itself, and `none` when nothing was sent — and the text says the same instead of always claiming a rollback.

Built in docs/plans/2026-10-07-transaction-outcome.md (crates/binsql-core/src/error.rs: Error::Transaction, TransactionOutcome; adapter/sqlx_common.rs and adapter/mssql.rs: run_transaction; crates/binsql/src/cli/mod.rs: Failure's transaction, caused, error_record; cli/exec.rs: the context line and commits_on_its_own; tests/sqlite_roundtrip.rs; tests/command_mode.rs; README's exec and Structured errors sections; HELP's ERRORS). No fake adapter exists, so the failed commit is checked for real: a SQLite deferred foreign key fails at the commit. The correctness review found that a rollback which succeeds after the batch's own `COMMIT`, or after MySQL DDL, may have undone nothing; such a batch now reports `unknown`. It also found that SQL Server reports `unknown` when the server has already rolled back by itself (a deadlock victim, `XACT_ABORT`), because the follow-up ROLLBACK fails; this is left as is, since it never claims a rollback that did not happen. The security review found nothing.

Assumed, not asked: only an error raised after BEGIN carries an outcome; any other, a failed BEGIN included, is `none`.
Assumed, not asked: a failed commit, or the failed rollback that ends a `--dry-run`, is `unknown` with no `statement`.
Assumed, not asked: a cancel inside a SQL Server batch is `unknown`.
Assumed, not asked: any control statement in the batch, `SET` and `USE` included, makes a rollback `unknown`, since the lexer does not tell `COMMIT` from them.
Assumed, not asked: `exec --no-tx` has no `transaction` field; it has `completed`.
