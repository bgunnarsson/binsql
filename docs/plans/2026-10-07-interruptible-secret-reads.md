---
title: "`az` and keychain reads can be interrupted"
date: 2026-10-07
status: done
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
- Both `az` calls go through one helper, `az::output`, which puts `az` in a
  process group of its own on unix and kills the group when the read is
  dropped: `az` is usually a shell script that runs Python without `exec`,
  so killing the direct child alone leaves the work running. It pipes
  stdout and stderr and closes stdin, as `Command::output()` did.
- The stub records the pid of the sleep it starts, without `exec`, in a file
  the script names, and the test checks it with `ps`, a zombie counting as
  gone. The same stub answers two secrets at once, so the test also shows a
  read that finishes returns `az`'s output and its refusal.
- Trade-off: in its own group, `az` no longer gets the terminal's SIGINT. If
  ⌃C ends binsql while `az` runs, `az` finishes its one request alone. The
  TUI is unaffected, since ⌃C is a key there.
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
  once a stub `az` has started its sleep, leaves the sleep dead within 2 s.
- A read that finishes still returns what `az` printed, and a refusal still
  carries `az`'s stderr.
- Both `az` calls run through `az::output`; nothing else in either function
  changes.
- A keychain reference is read inside `spawn_blocking`; its errors, the
  `NoEntry` hint included, come through unchanged.
- The existing resolver and keychain tests still pass.

## Tasks

- [x] **1. Kill `az` on drop, read the keychain off the runtime.**
  `az.rs`: `az::output`, killing `az`'s process group on drop.
  `secrets/azure.rs` and `adapter/mssql.rs`: call it.
  `secrets/mod.rs`: `keychain::get` in `spawn_blocking`.
  `tests/az_interrupt.rs`: the stub-`az` test.
  Verify: `cargo test --workspace`.
