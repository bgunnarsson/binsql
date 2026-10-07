---
title: "`--connect-timeout-ms` caps opening a connection"
kind: task
mode: afk
status: open
blocked_by: [50]
claimed_by:
---

## Question

Build what 11 settled for this step; the contract is in 11's answer
(`11-connect-budget.md`).

- Add a `Session` constructor that takes a resolved connection string. `open_with` calls it, and its behaviour is unchanged.
- Add `connect-timeout-ms` to `SHARED_VALUES`. Parse the flag, or `BINSQL_CONNECT_TIMEOUT_MS` when the flag is missing: absent or `0` means none, anything else that is not a number is exit 2.
- When a budget is set, `cli::connect` wraps loading, resolving and connecting in one deadline and reports which phase ran out with the `connect timeout:` message above.
- Tests in `crates/binsql/tests/command_mode.rs`:
  - A local `TcpListener` that accepts and never replies, used as `--dsn postgres://…` with `--connect-timeout-ms 300`, gives exit 1, `connect timeout:` and `while connecting` on stderr, empty stdout, and finishes in under 3 s.
  - A stub `az` that sleeps, a `keyvault://` source and `BINSQL_SECRET_TTL=0` give `while resolving the connection string`.
  - `--connect-timeout-ms abc` and the same value in the variable are exit 2.
  - A run with `0`, and a run with no flag, against SQLite produce output identical to today's.
- Update HELP under CONNECTION (`cli/mod.rs:91-97`) and README.md's command-mode flags with the flag, the variable, the meaning of `0`, what is covered, and that timeouts from driver settings are left alone.

## Answer
