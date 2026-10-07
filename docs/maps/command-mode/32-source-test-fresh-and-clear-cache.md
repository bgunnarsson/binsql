---
title: "`source test`, `--fresh` and `clear-cache`"
kind: task
mode: afk
status: resolved
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

Built: `binsql source test [NAME] [--fresh]` prints `name, ok, stage, elapsed_ms, error` and exits 1 at the failing stage (secret, token, connect); `binsql source clear-cache` deletes the secret cache and its key.

Plan: docs/plans/2026-10-07-source-test.md. Core: `Resolver::resolve_fresh`, `Session::probe` with `Stage` and `ProbeFailure`; the error has the DSN and the password it hides masked out. `default_source` is shared with `connect`.

Assumed, not asked: stage and error are null on success — a null holds up in every format. The keychain case of the secret-stage test is omitted — the OS keychain has no seam. Errors are masked by replacing the resolved DSN and the span masking hides — one rule for every adapter.

Review: two findings, both fixed (a secret-stage error quoting a vault URL's password; a DSN reading `azure ad` classed as the token stage). Pre-existing, unfixed: `binsql query --conn` prints the resolver's error unmasked on stderr, so a vault URL holding a password shows there. A refused port takes about 30s to report, as sqlx retries until its acquire timeout.

Live check not run: `--fresh` against a real Key Vault (no Azure here).
