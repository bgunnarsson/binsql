---
title: "`--timeout-ms` for query, with a bounded cancel"
date: 2026-10-07
status: done
---

## Context

Ticket [60](../maps/command-mode/60-timeout-ms-for-query-with-a-bounded-cancel.md)
is the first build step of 12's statement budget. Today the only stop in
command mode is ⌃C, so a query that never ends holds a script for as long as
the server cares to run it. Outcome: `query --timeout-ms N` cancels the
statement N ms after the connection opens, gives the cancel 2 s to land, and
exits 1 with `timed out after N ms`.

Settled: unattended loop; work stays on `main`, each task committed and pushed;
no AI mentions in commits. Additions only: without the flag, or with `0`,
output and exit codes stay as they are.

## Assumed, not asked

- The flag is in `query`'s own list, not `SHARED_VALUES`, until 61 and 62
  give `exec` and `inspect` a use for it: a verb should not accept a flag it
  ignores. 61 or 62 moves it to the shared list.
- The timeout gets its own category, `timeout`, phase `execute`, apart from
  `cancelled` and `connect-timeout`, as 12 settled once 10 had landed.
- The stop helper is a `Stop` built after connecting, holding the token and
  the deadline. Which one fired is told by where `run` is when it stops: a
  cancel the work reports before the deadline is ⌃C's and keeps today's
  message; one that the deadline caused is the timeout, whatever the work
  says afterwards.
- A query that finishes inside the grace prints its result and exits 0.
- The adapter error that comes back after the deadline, such as SQLite's or
  PostgreSQL's cancel error, is the record's `detail` and an indented line in
  text, never the message.
- `query --allow-write` running a statement that writes says whether it took
  effect is unknown, rather than "nothing was changed by binsql", which only
  a read can promise.
- `--plan` runs under the deadline too: it is work after connecting.

## Relevant lore

None in `docs/solutions`. From the map: tests never reach `ratatui::init`;
never name a zsh variable `status`; every new flag or category is in HELP and
the README.

## Acceptance criteria

- On SQLite, an endless recursive CTE with `--timeout-ms 200` exits 1 in
  under 200 ms plus the 2 s grace plus start-up, with empty stdout and stderr
  starting `error: timed out after 200 ms`; in JSON, category `timeout`,
  phase `execute`.
- `--timeout-ms abc` and `--timeout-ms 2147483648` are exit 2; `0` and a
  roomy value print exactly what no flag prints.
- `cargo test --workspace` passes; clippy is clean.

## Tasks

- [x] **1. The deadline.** `cli/mod.rs`: the `timeout-ms` parser, `Stop`
  replacing `cancel_on_interrupt` for `query`, `Category::Timeout`.
  `cli/query.rs`: run and plan through it.
  Verify: `cargo test --workspace` — unit tests for the parser, integration
  tests for the CTE, the bad values and the unchanged output.

- [x] **2. HELP and the README.** The flag under QUERY and in the README's
  command-mode flags: it excludes connecting, the grace is 2 s, a timeout is
  not proof of rollback; the category in Structured errors.
  Verify: `cargo test -p binsql`.
