---
title: "inspect under the deadline"
kind: task
mode: afk
status: resolved
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

`inspect --timeout-ms N` drops its metadata reads N ms after the connection
opens and exits 1 as `timed out after N ms`, category `timeout`, with nothing
on stdout and "nothing was changed by binsql". One deadline covers the
catalogue lookup, `list`, `describe` and `--columns`, and the connection
`--catalog` opens inside them. `timeout-ms` is now a shared flag, read by
`query`, `exec` and `inspect` alike; `source` still refuses it.

Built in docs/plans/2026-10-07-inspect-timeout.md (cli/mod.rs, cli/inspect.rs,
cli/query.rs, cli/exec.rs). Both reviews found nothing.

Assumed, not asked:
- `timeout-ms` moves to `SHARED_VALUES`; HELP keeps it per verb, since each verb says something different when it runs out.
- `inspect` does not go through `Stop`, which would install a ⌃C handler nobody answers; `cut_off` drops the work at the deadline (12's grace of 0) and ⌃C stays as it is.
- The failure says "nothing was changed by binsql" and adds no grace line.
- The timeout test builds 3000 tables so `inspect --columns` cannot finish in 1 ms.
