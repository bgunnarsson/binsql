---
title: Command mode is an agent's tool for finding, managing and querying data sources
date: 2026-10-07
status: active
---

## Destination

`binsql` without the TUI is a tool built mainly for agents. An agent can find
the data sources there are, add, edit and remove them, and connect to one,
from the command line alone. It finds and stores them exactly as the TUI
does: the user config, the nearest `.binsql.json`, folders and qualified
names, the keychain, and Key Vault references through `az`. The commands
themselves are designed fresh for agents, not carried over from v2. Beyond
managing data sources, command mode carries the ergonomics an agent leans on,
each chosen by research rather than guessed. Not part of it: anything with a
pane, new database drivers, new ways of looking up or authenticating to a data
source, and breaking changes to `query`, `exec` and `inspect`.

## Notes

- Agents are the main users. Where an agent and a person at a terminal want
  different things, the agent wins.
- Data sources are looked up exactly as the TUI does. The CLI only adds
  commands on top of that.
- The person prefers building over long upfront design talk: decide routine
  calls yourself and flag the gaps afterwards.
- `query` reads, `exec` writes, `inspect` describes. That split is a safety
  boundary (`crates/binsql/src/cli/mod.rs`), and new verbs must not blur it.
- Argument parsing stays hand-rolled in `crates/binsql/src/cli/args.rs`; it is
  deliberately small rather than a parser dependency. A ticket that finds it
  too small says so and makes the case.
- Stdout carries only the `--format` output. Notes go to stderr. Exit codes are
  0 success, 1 the database said no, 2 a usage mistake.
- Config writes go through `Workspace` (`crates/binsql-core/src/workspace.rs`),
  so user and project scope, owner-only writes and keychain moves behave the
  way they do behind `⌃N`.
- The README documents every flag. A task that adds one updates `README.md`
  and the `HELP` text in `cli/mod.rs`, and removes its line from Status once
  that gap is closed.

## Decisions so far

None yet.

## Not yet specified

- **Building the data-source commands**: these become task tickets once 08
  settles their shape. Some may first need logic moved from the app into the
  core, depending on 07.
- **Agent-ergonomics features**: each one 03 and 04 rank as worth having
  becomes a ticket of its own (a grilling where its shape is the person's
  call, a task where it is not).
- **Errors a program can parse**: whether failures get a structured form on
  stderr, and how that sits beside the exit codes. This waits on 03.
- **README Status**: rewriting the section once the data-source commands land.

## Out of scope

- TUI work. The map is about the command line; the grid, tree and history in
  the app are separate.
- New drivers. The four backends stay as they are.
- Lookup and authentication beyond what the TUI does, such as managed
  identity or service principals for Key Vault. The CLI uses the TUI's
  methods.
- Basing anything on v2's `binsql conn`. The commands are designed fresh.
- Breaking changes to existing verbs, flags, exit codes or output formats.
  Scripts already depend on them. Additions only.
