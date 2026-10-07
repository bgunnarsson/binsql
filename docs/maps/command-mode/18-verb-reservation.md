---
title: Reserve verb names for new top-level data sources, and open any name with `binsql -- <name>`
kind: task
mode: afk
status: resolved
blocked_by: []
claimed_by:
---

## Question

Build what 05 decided for the names already spent, before `source` arrives:

- Move the verb list into `binsql-core` as the reserved names, and have
  `cli::VERBS` / `is_verb` (`crates/binsql/src/cli/mod.rs:46`) read it.
- `Workspace::set` (`crates/binsql-core/src/workspace.rs:177`) refuses an id
  with no folder that is a reserved name and is not already saved. A source
  already saved under such a name stays editable; a name inside a folder is
  never refused. The TUI's `⌃N` shows the refusal as it shows other save
  errors.
- `parse_args` (`crates/binsql/src/main.rs:236`) accepts `--`: the argument
  after it is the TUI target, never a verb, so `binsql -- query` opens a saved
  `query`.
- Document `--` in the `HELP` text in `main.rs` and in `README.md`.

Verify with tests: a new top-level `query` is refused, `folder/query` is not,
an existing top-level `query` can still be saved again, `binsql -- query`
parses as a target, and `main.rs`'s existing argument tests still pass.

## Context

- [05](05-verb-namespace.md) for the decision. The `source` verb itself is
  added by the data-source build tickets after 08, which add it to the
  reserved list then, with 05's dispatch rule: `source` goes to command mode
  only when a non-flag argument follows it, verified to leave bare
  `binsql source` on today's path, message and exit 1 included. This ticket leaves `is_verb`'s
  meaning for the three existing verbs unchanged.
- Additions only: no existing verb, flag or exit code changes.

## Answer

Built: a new top-level data source named `query`, `exec` or `inspect` is refused by `Workspace::set` (and before any secret is filed in the TUI), and `binsql -- <name>` opens any saved name.

Plan: [2026-10-07-verb-reservation](../../plans/2026-10-07-verb-reservation.md), done. Reviewed for correctness, security, conventions and simplicity: no findings. Not checked by hand: the TUI paths (`binsql -- query` opening the app, `⌃N` showing the refusal), which need an interactive terminal.

Commits:

- 8c33cb2 Verb reservation plan is done
- 030e899 README documents binsql -- <name> and the reserved verb names
- ce97413 binsql -- <name> opens a data source even when the name is a verb
- 72397e1 Saving a new data source named like a verb files no secret
- 03b9dcd cli::is_verb answers from the reserved names in core
- 9492c12 Workspace refuses a new top-level data source named like a verb
- ba91a6e Plan: verb names are reserved for new top-level data sources
