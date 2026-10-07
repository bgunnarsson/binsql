---
title: What do scripts and agents need from a SQL CLI that binsql's command mode lacks?
kind: research
mode: afk
status: open
blocked_by: []
claimed_by:
---

## Question

The destination asks for agent and script ergonomics chosen by research rather
than guessed. Survey what comparable command-line SQL tools offer to
non-interactive callers: `psql`, `sqlite3`, `usql`, `sq`, `duckdb`,
`mysql`/`mycli`, `sqlcmd`, and any CLI built for agents. Compare that with
binsql's command mode today. Cover at least:

- discovering what can be connected to (listing saved data sources);
- failures a program can parse, and exit-code conventions;
- statement and connect timeouts;
- schema dumps and DDL output;
- `EXPLAIN` / plans;
- shell completions;
- a `--version` or capability probe that an agent can parse;
- streaming large results, and `--limit` behaviour;
- a non-zero exit on an empty result, or on an assertion.

Return a ranked candidate list. For each candidate, say what it would add, what
it would cost, and whether it bends the query/exec/inspect safety split. Mark
the ones whose shape is the person's call rather than obvious. These become the
map's next tickets.

## Context

- `README.md` "Command mode", `crates/binsql/src/cli/` (all of it, about 1,850
  lines), and `HELP` in `cli/mod.rs`.
- Out of scope per `MAP.md`: TUI features, new drivers, breaking changes.

## Answer
