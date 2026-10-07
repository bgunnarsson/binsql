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

Spend one bare verb, `source`, on all data-source management (`binsql source <op>`); every other addition is a flag or lives under an existing verb; `source` dispatches to command mode only when a non-flag argument follows it, so every `binsql source …` that worked before still opens the TUI; refuse to save a new top-level data source named after a verb; and add `binsql -- <name>` to open any saved name in the TUI.

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
   needs a ticket that argues it against a flag or a sub-op first, and must
   take only forms that were errors before, as `source` does below.
2. **`source` takes only forms that were errors.** `main.rs` sends `source`
   to command mode only when the argument after it exists and does not start
   with `-`. Before this, `binsql source <word>` always failed in `parse_args`
   with "Unexpected argument" (`main.rs:265`), since the target was already
   set; so that form is free. `binsql source` alone, or followed only by
   options (`binsql source -d postgres`, `binsql source --help`), keeps the
   TUI path exactly as today: it opens whatever `Config::resolve`
   (`binsql-core/src/config.rs:283`) finds for `source`, a top-level `source`
   or a unique `folder/source` by its leaf. Only when `source` resolves to
   nothing does bare `binsql source` print the `source` usage and exit 2, where
   today it fails with "not a saved data source" (`main.rs:58`): an error
   message reworded, the exit staying a failure. No invocation that succeeded
   before changes meaning, so the map's no-breaking-changes rule holds without
   an exception. `query`, `exec` and `inspect` already shadow their names,
   folder leaves included; that is existing behaviour and stays.
3. **Reserve, but only for new top-level names.** `Workspace::set`
   (`binsql-core/src/workspace.rs:177`) refuses an id that is a verb, has no
   folder, and is not already saved. Inside a folder any name is fine. Editing
   an existing source with a verb's name still works, so no config the person
   already has stops being editable. The reserved list moves into
   `binsql-core` so the TUI's `⌃N` and `source add` refuse the same names, and
   `cli::VERBS` reads it from there.
4. **An explicit escape, spending no name.** `binsql -- <name>` treats the next
   argument as a TUI target, never a verb: the usual end-of-options marker, so
   no `open` verb and no new flag. `parse_args` (`main.rs:236`) currently
   rejects `--` as an unknown option, so this is an addition, not a change. It
   is how a saved `query`, `exec` or `inspect`, top-level or a unique folder
   leaf, opens by name.
5. **Existing configs.** Nothing is renamed, migrated or refused on load.
   `source list` marks every entry whose bare name a verb takes: a top-level
   entry named after a verb, and a folder entry whose leaf is a verb and
   unique, so `resolve` would have matched it by the bare name
   (`config.rs:283`, `leaf_matches`). For each it says on stderr how to open
   it: `binsql -- <name>`, the qualified `folder/name`, or `--conn`. For a
   `source` entry the note says the bare name still opens it.

### Flagged for the person

Decided without a grilling session, per the map's note to decide routine calls
and flag them afterwards. Worth overruling if wrong: the verb's name (`source`
over `conn`, `sources` or `ds`). The first answer bent the no-breaking-changes
rule for a saved `source`; the checker found that needed your say-so, and
decision 2 now keeps every working invocation instead, so no exception is
asked for.
