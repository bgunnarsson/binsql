---
title: Let query fail an opt-in row assertion
kind: task
mode: afk
status: resolved
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

`query --require-rows` prints the result as usual, then exits 1 with category `assertion`, phase `output`, when it had no rows; `--plan` with it is a usage error.

Built in docs/plans/2026-10-07-require-rows.md (cli/query.rs, cli/mod.rs,
tests/command_mode.rs, README, HELP). The security review found nothing. The
correctness review found that SQL Server drops the rows an `OUTPUT` or `EXEC`
write returns, so `--require-rows` fails such a write; that is an older adapter
gap, now noted in the README rather than fixed here.

Assumed, not asked:
- A new category, `assertion`, phase `output`: the query succeeded and printed.
- The message is "the query returned no rows (--require-rows)".
- `--require-rows` with `--plan` is a usage error, since a plan always has rows.
- A write that returns no result set fails the check, as an empty `SELECT` does.
- Under `--limit`, one fetched row is enough.
