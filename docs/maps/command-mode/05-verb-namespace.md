---
title: How does the verb list grow without shadowing saved data sources?
kind: grilling
mode: hitl
status: resolved
blocked_by: []
claimed_by:
---

## Question

`binsql <name>` opens a data source unless `<name>` is a verb (`is_verb` in
`cli/mod.rs`). Every verb added (`conn`, maybe `list` or `completions`) takes a
name away from data sources: a saved data source called `conn` would stop
opening by its bare name. Decide with the person how the namespace grows:

- reserve verbs and refuse to save a data source by those names;
- group new verbs under one, such as `binsql conn …`, so only one name is spent;
- give the TUI an explicit `binsql open <name>` and leave bare names as a
  fallback;
- some combination of these.

Also decide what happens to an existing config that already uses a name that
becomes a verb, given that breaking changes are out of scope.

## Context

- `crates/binsql/src/main.rs` (how the first argument is dispatched) and
  `cli/mod.rs` (`VERBS`, `is_verb`).
- Ticket 03, if it is resolved by then, for which verbs are likely coming.

## Answer

Spend one bare verb, `source`, on all data-source management (`binsql source <op>`); every other addition is a flag or lives under an existing verb; refuse to save a new top-level data source named after a verb; add `binsql -- <name>` to open any saved name in the TUI; and leave existing configs loading and listed, with the verb winning the bare name.

### What it rests on

- Shadowing only happens in one place. `main.rs:40` sends the first argument
  to command mode when `cli::is_verb` says so (`cli/mod.rs:46`, `:52`);
  otherwise it is the TUI's target (`main.rs:265`). Command mode names its
  data source through `--conn`/`-c` or `BINSQL_CONN` (`cli/mod.rs:172`), so a
  verb never shadows anything an agent passes there. The cost of a verb falls
  only on a person typing `binsql <name>` to open the TUI.
- A qualified name is never shadowed: verbs contain no `/`, and `folder/name`
  splits on it (`binsql-core/src/config.rs:55`, `:67`). Only top-level names,
  and bare names that `resolve` widens to a unique folder entry, can collide.
- 03's ranking needs one new bare verb at most. Data-source list/show/test and
  writes (rank 1) are the only new command family. Capabilities are a global
  flag (14), row assertions a query flag (15), DDL and schema context belong to
  `inspect` (rank 5, 9), streaming to `query` (rank 6), plans to `inspect` or a
  read-only query flag (rank 10). Shell completions are out of scope.

### Decisions

1. **One verb for the family.** Data-source operations go under `binsql source
   <op>` (`source list`, `source show <name>`, …; 08 settles which ops exist).
   `source` matches the map's and the README's word, "data source". The bare
   verb list becomes `query`, `exec`, `inspect`, `source`. Any later bare verb
   needs a ticket that argues it against a flag or a sub-op first.
2. **Reserve, but only for new top-level names.** `Workspace::set`
   (`binsql-core/src/workspace.rs:177`) refuses an id that is a verb, has no
   folder, and is not already saved. Inside a folder any name is fine. Editing
   an existing source with a verb's name still works, so no config the person
   already has stops being editable. The reserved list moves into
   `binsql-core` so the TUI's `⌃N` and `source add` refuse the same names, and
   `cli::VERBS` reads it from there.
3. **An explicit escape, spending no name.** `binsql -- <name>` treats the next
   argument as a TUI target, never a verb: the usual end-of-options marker, so
   no `open` verb and no new flag. `parse_args` (`main.rs:236`) currently
   rejects `--` as an unknown option, so this is an addition, not a change.
4. **Existing configs.** Nothing is renamed, migrated or refused on load. A
   top-level source already named `source` keeps opening from the sidebar,
   by `binsql -- source`, and by `--conn source` in command mode; only the bare
   `binsql source` changes meaning, as `query`, `exec` and `inspect` already
   did. `source list` marks such an entry and says on stderr how to open it,
   so the collision is visible rather than silent. This is the one place the
   map's "no breaking changes" bends, and only for a top-level data source
   literally named `source`.

### Flagged for the person

Decided without a grilling session, per the map's note to decide routine calls
and flag them afterwards. Worth overruling if wrong: the verb's name (`source`
over `conn`, `sources` or `ds`), and whether a person already has a top-level
data source named `source` in their user config (not checked: it is outside
the project).
