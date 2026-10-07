---
title: Classify an executing EXPLAIN as the statement it runs
kind: task
mode: afk
status: open
blocked_by: []
claimed_by:
---

## Question

Fix what 17 found in `classify` (`crates/binsql-core/src/sql.rs:174`):
`EXPLAIN ANALYZE <write>` is a read today, so it passes `query`'s write
refusal (`crates/binsql/src/cli/query.rs:48`) and a read-only source's
guard (`Session::guard_read_only`, `crates/binsql-core/src/session.rs:177`),
and on PostgreSQL and MySQL it runs the write.

- `EXPLAIN ANALYZE <stmt>`, `EXPLAIN ANALYSE <stmt>` (PostgreSQL's spelling),
  `EXPLAIN VERBOSE ANALYZE`-style option runs, and `EXPLAIN (… ANALYZE …)`
  with `ANALYZE` not followed by `FALSE`/`OFF`/`0` take the kind of the
  wrapped statement, classified as `classify` would classify it alone.
  MySQL's `DESCRIBE`/`DESC ANALYZE` likewise.
- A plain `EXPLAIN`, `EXPLAIN QUERY PLAN`, `EXPLAIN FORMAT=… ` and
  `EXPLAIN (FORMAT JSON)` stay reads whatever they wrap: they do not run it.
- Tests in `sql.rs` beside the existing table (:617): each form above,
  with a leading comment, with a `WITH … DELETE` inner statement, and the
  existing `explain select 1` still a read.
- A test in the CLI or session that `query` refuses
  `EXPLAIN ANALYZE DELETE FROM t` with exit 2 and that a read-only source
  refuses it.

No output format, flag or exit changes beyond refusing these statements.

## Context

- 17's answer (`17-safe-plan-support.md`) has the backend forms and links.

## Answer
