---
title: `inspect --columns <name>` selects exactly one object
kind: task
mode: afk
status: resolved
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

Built: `inspect --columns <name>` selects exactly one object: the match is case-sensitive, never split on dots, and the schema comes only from `--schema` (plan docs/plans/2026-10-07-inspect-columns-name.md).

No match exits 1 with "no table or view named …", and names that differ only in case are appended in byte order. Exact matches in more than one schema exit 2, listing the schemas in byte order and asking for `--schema`. Plain `inspect <name>` keeps its case-insensitive match and its dot split. Checked by unit tests of `select` (an exact match, the case hint, two schemas, a dotted name) and by SQLite tests: an exact name, `ARTIST` failing with the hint and empty stdout, a table named `a.b`, and plain `inspect ARTIST` unchanged.

- Assumed, not asked: the case-only hint goes into the exit-1 message itself, not a separate note, because a failure carries one message.
- Review: the hint and the schema list came out in database order where the plan said byte order. Fixed in the commit after the build.
- Not checked live: ambiguity across schemas, which the SQLite fixture cannot produce. The unit test covers it.
