---
title: How does the verb list grow without shadowing saved data sources?
kind: grilling
mode: hitl
status: open
blocked_by: [1]
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
- Ticket 01, for the names v2 spent.
- Ticket 03, if it is resolved by then, for which verbs are likely coming.

## Answer
