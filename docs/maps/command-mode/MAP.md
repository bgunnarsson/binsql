---
title: Command mode does everything a script, CI job or agent needs without the TUI
date: 2026-10-07
status: active
---

## Destination

`binsql` without the TUI is enough on its own for a script, a CI job or an
agent. The command-line gaps the README's Status section lists against v2 are
closed: data sources can be added, edited, listed and removed from the command
line, in the user config or the project's `.binsql.json`, with the keychain as
`⌃N` uses it; and a Key Vault reference resolves in CI through managed identity
or a service principal, without `az login`. Beyond parity, command mode carries
the ergonomics an agent or script leans on, each chosen by research rather than
guessed: things like listing data sources, failures a program can parse,
timeouts, a schema dump an agent can read, and shell completions. Not part of
it: anything with a pane (grid editing, tree filtering, the TUI's query
history), new database drivers, and changes that break the verbs, flags, exit
codes or output formats scripts already depend on.

## Notes

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

- **Building `binsql conn`**: add, edit, list, remove, default. These become
  task tickets once 05 settles their shape.
- **Building the Key Vault credential chain**: which credentials, in what
  order, and how a CI job selects one. These become tasks once 06 picks the
  implementation.
- **Agent-ergonomics features**: each one 03 and 04 rank as worth having
  becomes a ticket of its own (a grilling where its shape is the person's
  call, a task where it is not).
- **Errors a program can parse**: whether failures get a structured form on
  stderr, and how that sits beside the exit codes. This waits on 03.
- **README Status**: rewriting the section once the parity tasks land.

## Out of scope

- TUI work. The map is about the command line; the grid, tree and history in
  the app are separate.
- New drivers. The four backends stay as they are.
- Breaking changes to existing verbs, flags, exit codes or output formats.
  Scripts already depend on them. Additions only.
