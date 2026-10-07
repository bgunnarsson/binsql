---
title: "`source remove`"
kind: task
mode: afk
status: resolved
blocked_by: [20, 30]
claimed_by:
---

## Question

Build what 08 settled for this step; the contract is in 08's answer
(`08-data-source-commands.md`).

- Requires `--force`, else exit 2. Uses ticket 20's delete: config first, then the secret. Print the removed row.
- Tests: no `--force` is exit 2; an unknown name is exit 2; the source is gone from whichever file held it.
- HELP and README.

## Answer

binsql source remove NAME --force deletes a data source from every file that holds it, then its keychain secret, and prints the row it had.

Built in docs/plans/2026-10-07-source-remove.md (crates/binsql/src/cli/source.rs: fn remove and the remove arms). Without --force or for an unknown name it exits 2 and writes nothing; when the secret cannot be deleted it exits 1 after the row, with the data source already gone. Review fix: Workspace::delete now deletes the secret of every file's entry, so a project entry shadowing a keychain-backed user one no longer leaves its secret behind (the TUI's delete gains this too).

Assumed, not asked: a name defined in both files is removed from both, as ticket 20's delete does, and the row printed is the one that resolved — the alternative needs a --scope on remove that 08 did not settle.
Assumed, not asked: a secret that cannot be deleted exits 1 after printing the row — the config change stands, and an agent has to see the leftover secret.
