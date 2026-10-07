---
title: "`--connect-timeout-ms` caps opening a connection"
date: 2026-10-07
status: done
---

## Context

Ticket [51](../maps/command-mode/51-connect-timeout-ms-caps-opening-a-connection.md)
builds 11's connect budget on top of 50, which made `az` and keychain reads
interruptible. Today `cli::connect` has no deadline, so a server that accepts
and never answers, or an `az` that hangs, holds a script for as long as the
driver or `az` cares to wait. Outcome: `--connect-timeout-ms N` (or
`BINSQL_CONNECT_TIMEOUT_MS`) caps loading, resolving and connecting at `N` ms,
and running out is exit 1 with `connect timeout: … while <phase>`.

Settled: unattended loop; work stays on `main`, each task committed and pushed;
no AI mentions in commits. Additions only: without the flag, or with `0`,
output and exit codes stay as they are.

## Assumed, not asked

- Ticket 10 has landed, so the timeout gets its own category,
  `connect-timeout`, phase `connect`, as 11 planned for that case.
- The core constructor is `Session::connect(name, source, dsn)`, taking the
  resolved connection string; `open_with` resolves, then calls it.
- The deadline is fixed once, before the config loads, and each async step
  runs under `timeout_at` that deadline. A step that is ready when polled
  still succeeds after the deadline: the config load cannot be interrupted,
  and a plain connection string resolves without waiting.
- The flag is parsed before the config loads, so a bad value is exit 2 even
  when the config is broken.
- A bad `BINSQL_CONNECT_TIMEOUT_MS` is trimmed first, and its message names
  the variable rather than the flag.
- `source test` keeps its own flag list and does not take the budget: it
  reports how long each stage took rather than stopping one.
- A secret or connect failure that arrives before the deadline keeps today's
  message and category.

## Relevant lore

None in `docs/solutions`. From the map: tests never reach `ratatui::init`;
never name a zsh variable `status`; every new field or category is in HELP and
the README.

## Acceptance criteria

- `--dsn postgres://…` at a listener that accepts and never replies, with
  `--connect-timeout-ms 300`, is exit 1, `connect timeout:` and
  `while connecting` on stderr, empty stdout, in under 3 s; in JSON, category
  `connect-timeout`, phase `connect`.
- A stub `az` that sleeps, a `keyvault://` source and `BINSQL_SECRET_TTL=0`
  give `while resolving the connection string`.
- `--connect-timeout-ms abc`, and `abc` in `BINSQL_CONNECT_TIMEOUT_MS`, are
  exit 2.
- With `0`, and with no flag, a SQLite query prints exactly what it does
  today.

## Tasks

- [x] **1. The budget.** `session.rs`: `Session::connect`. `cli/mod.rs`:
  `connect-timeout-ms` in `SHARED_VALUES`, the budget parsed from the flag or
  the variable, `timeout_at` around resolving and connecting, and
  `Category::ConnectTimeout`.
  Verify: `cargo test --workspace` — the four integration tests above.

- [x] **2. HELP and the README.** The flag and the variable under CONNECTION
  and in the README's command-mode flags: what `0` means, what is covered,
  that driver timeouts are left alone; the category in Structured errors.
  Verify: `cargo test -p binsql`.
