---
title: "inspect under the deadline"
date: 2026-10-07
status: active
---

## Context

Ticket [62](../maps/command-mode/62-inspect-under-the-deadline.md) gives
`inspect` the budget 60 gave `query` and 61 gave `exec`. Today a catalogue
that answers slowly, or a `--catalog` connection that hangs, holds a script
until ⌃C. Outcome: `inspect --timeout-ms N` drops its metadata reads at the
deadline and exits 1 as a timeout, with nothing on stdout.

Settled: unattended loop; work stays on `main`, each task committed and pushed;
no AI mentions in commits. Additions only: without the flag, or with `0`,
output, messages and exit codes stay as they are.

## Assumed, not asked

- `timeout-ms` moves to `SHARED_VALUES` now that every verb that runs SQL
  takes it, and leaves `query`'s and `exec`'s own lists.
- `inspect` has no token to cancel, so it does not go through `Stop`: that
  would install a ⌃C handler nobody answers. A deadline-only helper,
  `within`, drops the work at the deadline, which is 12's grace of 0, and
  leaves ⌃C as it is today.
- One deadline covers everything after connecting: the catalogue lookup,
  `list`, `describe` and `--columns`, and the per-catalog connection
  `--catalog` opens inside them.
- The failure says "nothing was changed by binsql", as `query`'s read does,
  and adds no grace line, since there is no grace.
- The timeout test builds a SQLite file with enough tables that
  `inspect --columns` cannot finish in 1 ms.

## Relevant lore

None in `docs/solutions`. From the map: tests never reach `ratatui::init`;
never name a zsh variable `status`; every new flag is in HELP and the README.

## Acceptance criteria

- On SQLite, `inspect`, `inspect artist` and `inspect --columns` with a roomy
  `--timeout-ms`, and with `0`, print byte-for-byte what no flag prints.
- On a SQLite file of a few thousand tables, `inspect --columns
  --timeout-ms 1` exits 1 with empty stdout and `timed out after 1 ms`; in
  JSON, category `timeout`.
- `query` and `exec` still take the flag; `--timeout-ms abc` on `inspect` is
  exit 2.
- `cargo test --workspace` passes; clippy is clean.

## Tasks

- [ ] **1. The deadline.** `cli/mod.rs`: `timeout-ms` in `SHARED_VALUES`,
  `within`. `cli/inspect.rs`: the work after connecting under it.
  `query.rs`, `exec.rs`: the flag out of their own lists.
  Verify: `cargo test --workspace` — the integration tests above.

- [ ] **2. HELP and the README.** The flag moves to the shared flags in HELP
  and the README, saying what each verb does when it runs out.
  Verify: `cargo test -p binsql`.
