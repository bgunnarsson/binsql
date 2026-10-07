---
title: "`az` and keychain reads can be interrupted"
kind: task
mode: afk
status: open
blocked_by: []
claimed_by:
---

## Question

Build what 11 settled for this step; the contract is in 11's answer
(`11-connect-budget.md`).

- Add `.kill_on_drop(true)` to the `az` commands in `secrets/azure.rs:33` and `adapter/mssql.rs:706`. Nothing else in either function changes.
- In `Resolver::resolve` (`secrets/mod.rs:65-67`), run `keychain::get` in `tokio::task::spawn_blocking` and turn a join error into `Error::config`.
- Tests:
  - A `#[cfg(unix)]` test puts a stub `az` on `PATH` that writes its pid and sleeps. It drops `azure::fetch` under a 100 ms `tokio::time::timeout` and asserts the pid is gone within 2 s.
  - The existing resolver tests still pass.
- No flags, so no change to README.md or HELP.

## Answer
