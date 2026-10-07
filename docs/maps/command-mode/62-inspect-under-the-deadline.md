---
title: "inspect under the deadline"
kind: task
mode: afk
status: open
blocked_by: [60]
claimed_by:
---

## Question

Build what 12 settled for this step; the contract is in 12's answer
(`12-statement-budget.md`).

- Wrap `list` and `describe` (`inspect.rs:34-37`) in the stop helper with a grace of 0. The metadata reads keep their current signatures and are dropped at the deadline.
- Tests: on SQLite, a large timeout gives output byte-for-byte the same as without the flag; the timeout path gives exit 1 with empty stdout.
- Coordinate with 25 and 26, which also edit `inspect.rs`, so `--columns` reads get the same treatment.
- Update the README and HELP to say the flag applies to inspect.

## Answer
