---
title: Can the CLI manage data sources through the core as the TUI does, or is that logic locked in the app?
kind: research
mode: afk
status: claimed
blocked_by: []
claimed_by: lead
---

## Question

The CLI looks up and stores data sources the same way the TUI does. Before
designing its commands, establish where that logic lives. Trace everything
the TUI does to list, add, edit, rename, move between the user and project
files, remove, and set the default for a data source. For each step, say
whether it runs through `binsql-core` (`Workspace`, `Config`, `secrets`), and
so is callable from the CLI as it stands, or is done in the app layer
(`crates/binsql/src/app`, `ui`), and so would have to move into the core
first. That includes the `⌃N` form's rules: which file a new data source goes
to, keychain storage by default, rename moving the keychain entry, and
validation of names and drivers.

Also list what an agent would need that the TUI never does: listing data
sources with their scope and masked DSN, showing one, testing a connection
without running SQL.

## Context

- `crates/binsql-core/src/workspace.rs`, `config.rs`, `secrets/keychain.rs`,
  `secrets/mod.rs`.
- The `⌃N` / edit / disconnect handling under `crates/binsql/src/app` and
  `crates/binsql/src/ui`.
- `crates/binsql/src/cli/mod.rs`, `connect`, which already resolves names
  through `Workspace`.
- README: "Use", "Project data sources", "The keychain".

## Answer
