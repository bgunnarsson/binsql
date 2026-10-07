---
title: "`--timeout-ms` for query, with a bounded cancel"
kind: task
mode: afk
status: open
blocked_by: []
claimed_by:
---

## Question

Build what 12 settled for this step; the contract is in 12's answer
(`12-statement-budget.md`).

- Add `timeout-ms` to `SHARED_VALUES` (`mod.rs:138`), and a parser for it: absent or 0 means off, a value that is not a number or is above 2147483647 is exit 2.
- Add a stop helper in `cli/mod.rs` to replace `cancel_on_interrupt`. It holds a token cancelled by ⌃C or by the deadline, records which one fired, and runs the work through a function that waits for the result, or for the deadline plus a grace argument (2000 ms). It reports a completed result as success, and a deadline that fired as the `timed out after N ms` failure (exit 1) with any adapter error as detail.
- `query` uses the helper (`query.rs:59-63`). The clock starts after `connect` returns.
- Tests: argument parsing (absent, 0, a number, a value that is not a number, a value that is too large); without the flag, the output is byte-for-byte unchanged; on SQLite, an endless recursive CTE with `--timeout-ms 200` exits 1 within the timeout plus the grace, with empty stdout and the stderr prefix.
- Update the README and HELP: the flag excludes connecting, the cleanup grace is 2 s, and a timeout is not proof of rollback.

## Answer
