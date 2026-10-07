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
pane, new database drivers, new places to look data sources up, and breaking
changes to `query`, `exec` and `inspect`.

## Notes

- Agents are the main users. Where an agent and a person at a terminal want
  different things, the agent wins.
- Data sources are looked up exactly as the TUI does. The CLI only adds
  commands on top of that.
- Nearly every database the person uses keeps its connection string in Azure
  Key Vault. Key Vault working well for an agent is central to the map, not an
  edge case.
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

- [03](03-agent-ergonomics-survey.md): Prioritize data-source discovery, structured errors, bounded waits and richer schema context; add opt-in streaming, capability discovery and row assertions, and decide DDL and safe plans separately while preserving existing defaults and the query/exec/inspect split.

## Follow-up tickets from 03

- [10 — Structured errors](10-structured-errors.md): settle opt-in stderr records.
- [11 — Connect budget](11-connect-budget.md): settle the connection deadline after 09.
- [12 — Statement budget](12-statement-budget.md): settle execution and cleanup deadlines.
- [13 — Streaming contract](13-streaming-contract.md): settle export and partial-output semantics.
- [14 — Capability probe](14-capability-probe.md): build an offline JSON manifest.
- [15 — Require rows](15-require-rows.md): build a narrow opt-in query assertion.
- [16 — DDL output](16-ddl-output.md): settle definition fidelity after 04.
- [17 — Safe plan support](17-safe-plan-support.md): research backends and EXPLAIN classification.

## Not yet specified

- **Building the data-source commands**: these become task tickets once 08
  settles their shape. Some may first need logic moved from the app into the
  core, depending on 07.
- **Richer schema context**: 03 ranks one-call schema context as worth having;
  its shape and build tickets wait on 04's per-backend inventory.
- **Estimated-plan implementation**: 03 identifies value and an existing
  EXPLAIN safety concern; shape and build tickets wait on 17's backend audit
  and 05's namespace decision. Runtime-plan policy needs the person's call.
- **Ergonomics implementation**: build tickets follow the contract decisions
  in 10 (errors), 11 (connect budget), 12 (statement budget), 13 (streaming)
  and 16 (DDL). Capability discovery and row assertions are tasks 14 and 15.
- **Key Vault credentials beyond `az`**: whether an agent ever runs where
  `az login` is not available, and what binsql should do then. This waits on
  09.
- **README Status**: rewriting the section once the data-source commands land.

## Out of scope

- TUI work. The map is about the command line; the grid, tree and history in
  the app are separate.
- New drivers. The four backends stay as they are.
- Basing anything on v2's `binsql conn`. The commands are designed fresh.
- Breaking changes to existing verbs, flags, exit codes or output formats.
  Scripts already depend on them. Additions only.

- Shell completions for this map: 03 found stronger agent needs in capability
  discovery and saved-source listing; no interactive shell requirement was
  established. Revisit in a separate map if requested.
