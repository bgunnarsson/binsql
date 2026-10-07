---
title: Let query fail an opt-in row assertion
kind: task
mode: afk
status: open
blocked_by: [3]
claimed_by:
---

## Question

Add `query --require-rows`. Preserve normal result formatting on stdout; after
a successful query returning zero rows, report an assertion failure on stderr
and exit 1. One or more rows remains exit 0. Without the flag, empty results
remain success. Preserve failure/usage exits, read-only enforcement, --limit
semantics and --format none. Use the existing text failure contract; if ticket 10's error mode has been
implemented by then, use its structured assertion category too. Document a predicate SELECT assertion and
warn that COUNT(*) returns a row even for zero count. Test empty and nonempty
results, quiet output, usage/database failures and unchanged defaults. Do not
add a scalar assertion language, arbitrary exit mapping or exec assertions.

## Context

Ticket 03, and 10 if resolved; `crates/binsql/src/cli/query.rs`, cli/mod.rs, render.rs,
README Command mode and command_mode integration tests. SQL predicates express
the condition; binsql only tests whether the successful result has rows.

## Answer
