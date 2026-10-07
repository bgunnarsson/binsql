---
title: "exec under the deadline: whole batch, honest messages"
kind: task
mode: afk
status: resolved
blocked_by: [60, 63]
claimed_by:
---

## Question

Build what 12 settled for this step; the contract is in 12's answer
(`12-statement-budget.md`).

- `exec` runs both the transactional and the non-transactional paths through the stop helper. The non-transactional loop checks the token before each statement and never starts one after the deadline.
- Timeout messages follow the contract above. The existing ⌃C and error messages are unchanged (`exec.rs:62-66, 74-86`).
- Tests on SQLite: a batch whose second statement runs too long reports "statement 2 of N" and "1 earlier statement already ran". A transactional batch that times out leaves the table unchanged, and its message does not contain "rolled back". `--dry-run` with the deadline does the same.

## Answer

`exec --timeout-ms N` bounds the whole batch from when the connection opens,
and exits 1 as `timed out after N ms`, category `timeout`. Without a
transaction no statement starts after the deadline: the batch says which
statement was running, whose outcome is unknown, or which was not started,
and how many before it ran and were kept. A transaction cut off is
`transaction: unknown` with 12's line about an in-flight `COMMIT`, and a
`--dry-run` says whether its rollback completed is unknown; neither claims a
rollback. A ⌃C keeps today's messages.

Built in docs/plans/2026-10-07-exec-timeout.md (cli/mod.rs, cli/exec.rs).
Both reviews found the same defect: the deadline's own cancel was read as a
⌃C, so after a statement finished inside the grace a `--no-tx` batch started
the next one unbounded and reported it as cancelled. `Stop` now records that
the deadline fired, with a unit test for that sequence. Nothing else was
found.

Assumed, not asked:
- The flag is in exec's own `VALUES`; 62 moves it to `SHARED_VALUES`.
- A statement past the deadline is not started: "statement K of M was not started".
- A statement cut off says "statement K of M was running; whether it took effect is unknown".
- A transaction cut off says 12's line and is `transaction: unknown`; a dry run says it sends no `COMMIT` but its rollback is unknown.
- A cancel wrapped in a transaction error counts as the deadline's cancel.
- ⌃C messages are unchanged.
