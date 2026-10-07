---
title: "`az` and keychain reads can be interrupted"
kind: task
mode: afk
status: resolved
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

Dropping a Key Vault or `fedauth=` token read kills `az` and everything it
started, and a keychain read runs on a blocking thread, so a deadline can cut
either.

Built in docs/plans/2026-10-07-interruptible-secret-reads.md (az.rs,
secrets/mod.rs, secrets/azure.rs, adapter/mssql.rs, tests/az_interrupt.rs,
Cargo.toml). The correctness review found that `kill_on_drop` killed only the
`az` shell wrapper, leaving its Python running (fixed: `az` gets its own
process group and the group is killed), and a race on the stub's pid file
(fixed). The security review found a predictable scratch directory (fixed:
created fresh, mode 0700). Both re-reviews then found that spawning `az`
directly had stopped capturing its output, so secrets went to binsql's stdout
and reads failed (fixed: stdout and stderr piped, stdin closed, and the test
now checks a read that finishes), and that the group kill could follow the
leader's reaping (fixed: only the final `wait` reaps it). ⌃C during connect no
longer reaches `az`, which then finishes its one request alone; accepted.

Assumed, not asked:
- Both `az` calls share one helper, `az::output`, which kills `az`'s process group on drop.
- The stub-`az` test is its own test binary in `binsql-core`, since it sets `PATH`.
- A keychain join error becomes `Error::config("reading the keychain: …")`.
- `fedauth=`'s `az` call has no test of its own; it runs through the same helper.
- In the CLI, ⌃C during connect leaves `az` to finish alone.
