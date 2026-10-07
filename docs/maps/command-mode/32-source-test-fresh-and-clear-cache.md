---
title: "`source test`, `--fresh` and `clear-cache`"
kind: task
mode: afk
status: open
blocked_by: [30]
claimed_by:
---

## Question

Build what 08 settled for this step; the contract is in 08's answer
(`08-data-source-commands.md`).

- A core probe that returns a `secret`, `token` or `connect` stage on failure. `Resolver::resolve_fresh`. `source clear-cache` calls `Cache::clear`.
- Tests:
  - The `secret` stage fails on a missing keychain entry or a bare vault URL (`secrets/mod.rs:73`).
  - A bad sqlite path fails the `connect` stage.
  - A good sqlite source is ok with exit 0.
  - `--fresh` skips a cached entry (using `Resolver::new` with a temporary cache directory).
  - `clear-cache` removes both files.
- HELP and README cover `test`, `--fresh`, `clear-cache` and `BINSQL_SECRET_TTL=0`.

## Answer
