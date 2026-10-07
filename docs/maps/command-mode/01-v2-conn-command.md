---
title: What did v2's `binsql conn` do, and how does each part map onto v3's Workspace?
kind: research
mode: afk
status: open
blocked_by: []
claimed_by:
---

## Question

v2 managed data sources from the command line with `binsql conn`. v3 dropped it
(README, Status). Before the replacement is shaped, establish what v2's verb
did: every subcommand, flag, prompt and output, how it handled secrets and
Key Vault references, and its tests. Then map each piece onto what v3 now has:
`Workspace` with user and project scope, keychain-backed DSNs, folders and
qualified names. List which parts carry over unchanged, which need a new
answer (for example, which file a new connection is written to), and which no
longer make sense.

## Context

- The v2 Go tree was deleted in b9d4e03. Read it from history:
  `git show b9d4e03^:_old/internal/cli/cmd_conn.go`, together with `cli.go`,
  `cli_test.go` and `session.go` in the same directory.
- v3: `crates/binsql-core/src/workspace.rs` (`set`, `remove`, `scope_of`,
  `default_scope`), `config.rs` (`qualify`, `resolve`, `mask_dsn`, `save_to`),
  `secrets/keychain.rs`.
- How `⌃N` writes a connection today: find the form under `crates/binsql/src/app`
  or `ui`, and the scope rule described in README "Project data sources".

## Answer
