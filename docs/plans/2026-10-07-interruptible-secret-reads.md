---
title: "`az` and keychain reads can be interrupted"
date: 2026-10-07
status: active
---

## Context

Ticket [50](../maps/command-mode/50-az-and-keychain-reads-can-be-interrupted.md)
is the first build step of 11's connect budget. Today a future that runs `az`
can be dropped while the child keeps running, and a keychain read blocks the
runtime thread it runs on, so a deadline could neither stop the one nor fire
over the other. Outcome: dropping the future kills its `az` child, and the
keychain read runs on a blocking thread, so 51's deadline has something to cut.

Settled: unattended loop; work stays on `main`, each task committed and pushed;
no AI mentions in commits. No flags, so HELP and the README stay as they are.

## Assumed, not asked

- The stub-`az` test lives in its own integration test file in
  `binsql-core`, as its only test, because it has to set `PATH` for the whole
  process; it reaches `azure::fetch` through `Resolver::resolve_fresh` with a
  zero-TTL cache, since the `azure` module is private.
- The stub records its pid in a file named by an environment variable the
  test sets, and the test checks the pid with `kill -0`.
- A join error from `spawn_blocking` becomes
  `Error::config("reading the keychain: …")`.
- `adapter/mssql.rs`'s `az account get-access-token` gets the same
  `kill_on_drop` without a test of its own: it is the same one-line change
  and has no seam a test could reach without a SQL Server.

## Relevant lore

None in `docs/solutions`. From the map: tests never reach `ratatui::init`;
never name a zsh variable `status`.

## Acceptance criteria

- On unix, `Resolver::resolve_fresh` on a `keyvault://` reference, dropped
  after 500 ms while a stub `az` sleeps, leaves no stub process alive within
  2 s.
- Both `az` commands are built with `kill_on_drop(true)`; nothing else in
  either function changes.
- A keychain reference is read inside `spawn_blocking`; its errors, the
  `NoEntry` hint included, come through unchanged.
- The existing resolver and keychain tests still pass.

## Tasks

- [x] **1. Kill `az` on drop, read the keychain off the runtime.**
  `secrets/azure.rs` and `adapter/mssql.rs`: `.kill_on_drop(true)`.
  `secrets/mod.rs`: `keychain::get` in `spawn_blocking`.
  `tests/az_interrupt.rs`: the stub-`az` test.
  Verify: `cargo test --workspace`.
