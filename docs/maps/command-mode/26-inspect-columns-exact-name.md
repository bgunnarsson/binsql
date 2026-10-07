---
title: `inspect --columns <name>` selects exactly one object
kind: task
mode: afk
status: open
blocked_by: [25]
claimed_by:
---

## Question

Build what 19 settled for this step (see `19-schema-context-contract.md`, the contract and "Build tickets"):

- Exact, case-sensitive match, no dot splitting, schema only from `--schema`.
- No match exits 1, listing near-matches that differ only in case. Ambiguity across schemas exits 2, listing the schemas.
- Tests:
  - a SQLite object named with a dot;
  - a case-only near-match, giving exit 1 and the stderr hint;
  - a unit test of ambiguity across two schemas on the matching function, giving exit 2 and empty stdout;
  - plain `inspect <name>` keeps today's case-insensitive behaviour and its dot split.
- README and HELP document the positional form under `--columns`.

## Context

- 19's answer is the contract; 04's answer has the adapter queries and line numbers.

## Answer
