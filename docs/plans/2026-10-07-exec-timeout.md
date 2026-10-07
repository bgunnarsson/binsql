---
title: "exec under the deadline: whole batch, honest messages"
date: 2026-10-07
status: active
---

## Context

Ticket [61](../maps/command-mode/61-exec-under-the-deadline-whole-batch-honest-message.md)
gives `exec` the budget 60 gave `query`. Today a batch that hangs holds a
script until ⌃C, and the messages a stop gets were written for an error the
database reported, so one says "the transaction was rolled back; nothing was
kept" when nothing confirmed it. Outcome: `exec --timeout-ms N` bounds the
whole batch, starts no statement after the deadline, and says only what is
known about what the batch left behind.

Settled: unattended loop; work stays on `main`, each task committed and pushed;
no AI mentions in commits. Additions only: without the flag, or with `0`,
output, messages and exit codes stay as they are.

## Assumed, not asked

- The flag joins `exec`'s own list; 62 moves it to `SHARED_VALUES` when
  `inspect` takes it too.
- Without a transaction each statement runs through `Stop::run` on the one
  deadline. A statement the deadline would start is not started: the batch
  stops with "statement K of M was not started", the earlier-statements line
  kept.
- A statement cut off says "statement K of M was running; whether it took
  effect is unknown", and keeps the `statement`, `completed` and statement
  summary that any failure there carries.
- A transaction cut off says 12's line and is `transaction: unknown`. A dry
  run sends no `COMMIT`, so it says "the dry run did not finish; it sends no
  COMMIT, but whether its rollback completed is unknown" instead.
- A cancel that comes back wrapped in a transaction error, as one inside a
  batch does, counts as the cancel the deadline asked for, not as detail.
- A ⌃C keeps today's messages, as 60 kept them for `query`.

## Relevant lore

None in `docs/solutions`. From the map: tests never reach `ratatui::init`;
never name a zsh variable `status`; every new flag or category is in HELP and
the README.

## Acceptance criteria

- On SQLite, `--no-tx` with a quick insert and then an endless statement and
  `--timeout-ms 200` exits 1 with "statement 2 of 2" and "1 earlier statement
  already ran".
- A transactional batch that times out leaves the table as it was, and its
  stderr does not contain "rolled back"; `--dry-run` likewise. In JSON,
  category `timeout`, `transaction: unknown`.
- `0` and a roomy value print exactly what no flag prints.
- `cargo test --workspace` passes; clippy is clean.

## Tasks

- [ ] **1. The deadline.** `cli/mod.rs`: `Stop::overdue`, a cancel inside a
  transaction error read as the cancel. `cli/exec.rs`: both paths through
  `Stop`, the timeout messages.
  Verify: `cargo test --workspace` — the integration tests above.

- [ ] **2. HELP and the README.** The flag under EXEC and in the README, with
  what each kind of batch says when it runs out.
  Verify: `cargo test -p binsql`.
