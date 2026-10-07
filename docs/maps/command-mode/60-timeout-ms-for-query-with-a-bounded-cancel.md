---
title: "`--timeout-ms` for query, with a bounded cancel"
kind: task
mode: afk
status: resolved
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

`query --timeout-ms N` cancels the statement N ms after connecting and exits 1
as `timed out after N ms`, category `timeout`, phase `execute`; a cancel the
backend has not acted on within 2 s is left behind and said so, and a write
under `--allow-write` says its outcome is unknown (`transaction: unknown` in
JSON) rather than that nothing changed.

Built in docs/plans/2026-10-07-query-timeout.md (cli/mod.rs, cli/query.rs).
The security review found nothing. The correctness review found that a ⌃C
the backend had not answered by the deadline was reported as a timeout, and
that the JSON record could not tell a timed-out write from a read; both were
fixed, and the database's code from the cancel's answer is kept on the record.

Assumed, not asked:
- The flag is in query's own `VALUES` for now, not `SHARED_VALUES`; it moves there when `exec` takes it (61).
- It gets its own `timeout` category, phase `execute`, not `cancelled`.
- The helper is `Stop`, which decides which stop fired; `cancel_on_interrupt` stays for `exec` until 61.
- A query that finishes inside the grace succeeds.
- The adapter's answer to the cancel goes into `detail` and a text context line.
- `--plan` runs under the deadline too.
- A cancel left behind after the grace is said only in the text context line; JSON shows it as a timeout like any other.
