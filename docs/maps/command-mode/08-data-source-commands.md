---
title: What commands does the CLI offer for data sources, shaped for an agent?
kind: grilling
mode: hitl
status: open
blocked_by: [5, 7]
claimed_by:
---

## Question

With the namespace settled (05) and the core's seams known (07), decide with
the person what the data-source commands are and how they behave for an
agent:

- which operations exist: list, show, add, edit, remove, set the default,
  test the connection;
- what each prints in each `--format`, and what it never prints, such as a
  plain-text DSN;
- how a connection string reaches `add` without landing in the agent's
  transcript or the shell history (stdin, an environment variable, or an
  interactive prompt only);
- how user and project scope are chosen, and what the default is when a
  `.binsql.json` is in play;
- whether writes need a confirming flag, given that the caller is an agent.

The answer should settle enough to cut the build into task tickets.

## Context

- Tickets 05 and 07, and 03 if it is resolved.
- `crates/binsql/src/cli/mod.rs` and `render.rs`, for the output conventions
  already in place.

## Answer
