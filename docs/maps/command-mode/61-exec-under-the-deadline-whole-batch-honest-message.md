---
title: "exec under the deadline: whole batch, honest messages"
kind: task
mode: afk
status: open
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
