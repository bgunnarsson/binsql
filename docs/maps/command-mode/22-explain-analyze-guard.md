---
title: Classify an executing EXPLAIN as the statement it runs
kind: task
mode: afk
status: resolved
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

Resolved: EXPLAIN, DESCRIBE and DESC that run their statement (ANALYZE/ANALYSE, bare or in PostgreSQL's option list, quoted or not) classify as that statement; a plain or ANALYZE-off EXPLAIN stays a read.

classify routes EXPLAIN/DESCRIBE/DESC to `explained` (crates/binsql-core/src/sql.rs), which walks the options with a new `lexemes` helper over `scan`; an option it does not know starts the wrapped statement, which then classifies as Unknown and is refused. Plan: docs/plans/2026-10-07-explain-analyze-classify.md. Tests: sql::tests an_executing_explain_takes_the_kind_of_what_it_runs, a_plain_explain_stays_a_read, lexemes_keep_digits_and_punctuation_and_where_they_start; command_mode query_refuses_to_write_and_refuses_a_script and a_read_only_data_source_refuses_a_write_from_the_command_line.

Review: two reviewers found, each on their own, that a quoted option name (`EXPLAIN ("analyze") DELETE ...`) read as a plain EXPLAIN; fixed in d4e27cf (lexemes now keep a quote's contents). They also found two defects in `scan` that were there before this change and fool every classification, not only EXPLAIN: PostgreSQL nested block comments end at the first `*/` (`/* a /* b */ SELECT */ DELETE FROM t` reads as a SELECT), and MySQL executable comments `/*! ... */` are treated as comments though MySQL runs them (`EXPLAIN /*!80018 ANALYZE */ DELETE ...`). Pre-existing, unfixed: the user decides.
