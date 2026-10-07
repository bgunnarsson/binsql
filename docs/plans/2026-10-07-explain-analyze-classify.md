---
title: An EXPLAIN that runs its statement is classified as that statement
date: 2026-10-07
status: in-progress
---

## Context

Ticket 22 (`docs/maps/command-mode/22-explain-analyze-guard.md`), from what
ticket 17 found (`17-safe-plan-support.md`): `classify`
(`crates/binsql-core/src/sql.rs:174`) calls any statement whose first word is
`EXPLAIN`, `DESCRIBE` or `DESC` a read (:181). `EXPLAIN ANALYZE DELETE …`
therefore passes `query`'s write refusal (`crates/binsql/src/cli/query.rs:48`,
usage error, exit 2) and a read-only source's guard
(`Session::guard_read_only`, `crates/binsql-core/src/session.rs:178`, which
classifies through `sql::split`), and on PostgreSQL and MySQL it runs the
write.

Outcome: an `EXPLAIN` (or MySQL `DESCRIBE`/`DESC`) that executes what it wraps
takes the kind of the wrapped statement, classified by `classify` as if it
stood alone. One that only plans stays a read whatever it wraps. No output,
flag or exit changes beyond refusing these statements.

Decisions already made (lead, the user being unavailable; the change that
moves least):

- The fix lives inside `classify`, so `query` and the read-only guard both get
  it with no change to either.
- The wrapped statement's kind comes from calling `classify` on the text after
  the `EXPLAIN` prefix.
- `ANALYZE` alone as a first word stays `Kind::Ddl`, as today (:198–202).
- It applies on every backend. SQLite and SQL Server have no executing
  `EXPLAIN`, so there it only refuses something the server would reject
  anyway (17's probe: SQLite answers `EXPLAIN ANALYZE` with a syntax error).

### Design

`words` (:270) keeps only alphabetic and `_` code characters, so it cannot
see `(`, `,`, `)`, `=` or `0`, which the option list needs. Add one small
tokeniser beside it, built on `scan` (:323), and leave `words` alone:

```rust
/// A bare word (letters, digits, `_`) or one punctuation character from the
/// executable text, and where in `text` it starts.
struct Lexeme { text: String, at: usize }

/// The statement's characters as `scan` gave them, and its lexemes.
fn lexemes(sql: &str, backend: Backend) -> (Vec<char>, Vec<Lexeme>)
```

- Every `Token::Char` goes into the `Vec<char>`, so `at` indexes characters
  as emitted, comments and literals included; the inner statement is
  `text[at..].iter().collect::<String>()`. Indexing the emitted characters
  rather than byte offsets in `sql` means no `len_utf8` arithmetic, and a
  `Token::BatchSeparator` (never present in what `split` hands over) ends
  the scan rather than desynchronising anything.
- A `Class::Code` alphanumeric or `_` extends the current word; any other
  non-whitespace code character is a one-character lexeme; whitespace,
  comments and literals end a word and produce nothing. Words are
  upper-cased.

`classify` sends `"EXPLAIN" | "DESCRIBE" | "DESC"` to a new
`fn explained(sql, backend) -> Kind`, taking them out of the `Read` arm.
After the first lexeme it reads the options:

1. **Parenthesised list** (PostgreSQL `EXPLAIN ( … )`): if the next lexeme is
   `(`, walk to the first `)`. It runs the statement when an `ANALYZE` or
   `ANALYSE` inside is not followed by `FALSE`, `OFF` or `0` (followed by
   `,`, `)`, `TRUE`, `ON`, `1` or anything else counts as on). The inner
   statement starts after the `)`.
2. **Bare options** otherwise (PostgreSQL's old form, MySQL, SQLite): skip
   lexemes while they are one of `ANALYZE`, `ANALYSE`, `VERBOSE`, `QUERY`,
   `PLAN`, `EXTENDED`, `PARTITIONS`, `FORMAT`, `=`, `TRADITIONAL`, `JSON`,
   `TREE`. It runs the statement when `ANALYZE` or `ANALYSE` was among them.
   The inner statement starts at the first lexeme not in that set.
3. Not run: `Kind::Read`, exactly as today. Run: `classify(&inner, backend)`.
   An empty or unrecognised inner statement comes back `Kind::Unknown`, which
   `mutates()`, so the guards refuse it.

Recursion terminates because the inner text is strictly shorter.

### Trade-offs

- A keyword reader, not a parser. Where it errs, it errs towards refusing:
  `EXPLAIN (ANALYZE 'false') DELETE …` is treated as executing because
  literals are not lexemes, and an option list it does not know ends the
  option run early, leaving an inner statement that classifies `Unknown`.
  Both are refusals of something harmless, never a write let through.
- `EXPLAIN ANALYZE SELECT f()` stays a read, as `SELECT f()` is: the general
  limit of keyword classification, noted in 17, not addressed here.
- A second tokeniser beside `words` duplicates a dozen lines; changing
  `words` to keep digits and punctuation would alter `has_keyword` and the
  `WITH` scan, which is a larger change than this ticket asks for.

## Relevant lore

None found. (The kept note about command-mode `--limit` does not bear on
classification.)

## Acceptance criteria

- `classify` returns `Kind::Write` for `EXPLAIN ANALYZE DELETE FROM t`,
  `explain analyse update t set a = 1`, `EXPLAIN VERBOSE ANALYZE INSERT …`,
  `EXPLAIN ANALYZE VERBOSE DELETE …`, `EXPLAIN (ANALYZE) DELETE …`,
  `EXPLAIN (FORMAT JSON, ANALYZE TRUE) DELETE …`,
  `EXPLAIN (ANALYZE, BUFFERS) DELETE …`, `EXPLAIN ANALYZE FORMAT=TREE DELETE …`,
  `DESCRIBE ANALYZE DELETE …`, `DESC ANALYZE UPDATE …`,
  `/* c */ EXPLAIN ANALYZE DELETE …` and
  `EXPLAIN ANALYZE WITH x AS (SELECT 1) DELETE FROM t`.
- `EXPLAIN ANALYZE SELECT 1` and `EXPLAIN (ANALYZE) WITH x AS (SELECT 1) SELECT * FROM x`
  are `Kind::Read`; `EXPLAIN ANALYZE CREATE TABLE t AS SELECT 1` is `Kind::Ddl`;
  `EXPLAIN ANALYZE` with nothing after it is `Kind::Unknown`.
- `explain select 1`, `EXPLAIN DELETE FROM t`, `EXPLAIN QUERY PLAN DELETE FROM t`,
  `EXPLAIN FORMAT=JSON DELETE FROM t`, `EXPLAIN (FORMAT JSON) DELETE FROM t`,
  `EXPLAIN (ANALYZE FALSE) DELETE FROM t`, `EXPLAIN (ANALYZE OFF) …`,
  `EXPLAIN (ANALYZE 0) …` and `DESCRIBE t` stay `Kind::Read`.
- `ANALYZE` and `analyze t` stay `Kind::Ddl`.
- `binsql query "EXPLAIN ANALYZE DELETE FROM artist WHERE id = 1"` exits 2
  with the existing write refusal, and the row is still there.
- `binsql exec --conn prod "EXPLAIN ANALYZE DELETE FROM artist WHERE id = 1" --force`
  on the read-only fixture fails with "read-only".
- Every existing test passes unchanged.

## Tasks

- [ ] Add `Lexeme` and `lexemes` to `sql.rs` under the `words` helper, with a
  unit test that `lexemes("EXPLAIN (ANALYZE 0) /* x */ DELETE 'a'", …)`
  yields `EXPLAIN ( ANALYZE 0 ) DELETE` and that the `at` of `DELETE`
  recovers `DELETE 'a'` from the text. Verify:
  `cargo test -p binsql-core sql::tests::lexemes`.
- [ ] Add `explained` and route `EXPLAIN`/`DESCRIBE`/`DESC` to it in
  `classify`; update `Kind::Read`'s doc comment (:18) and `classify`'s doc
  comment to say an executing `EXPLAIN` takes its statement's kind. Add the
  tests `an_executing_explain_takes_the_kind_of_what_it_runs` and
  `a_plain_explain_stays_a_read` beside `classifies_by_leading_keyword`
  (:613), covering every case in the acceptance criteria, PostgreSQL and
  MySQL forms with their backend. Verify:
  `cargo test -p binsql-core explain` and `cargo test -p binsql-core classif`.
- [ ] Add `EXPLAIN ANALYZE DELETE FROM artist WHERE id = 1` to
  `query_refuses_to_write_and_refuses_a_script`
  (`crates/binsql/tests/command_mode.rs:213`, `fixture.direct(...).refused()`,
  before the count check that proves nothing was written) and to the
  disguises loop in
  `a_read_only_data_source_refuses_a_write_from_the_command_line` (:334).
  Verify: `cargo test -p binsql --test command_mode refuse`.
- [ ] Resolve ticket 22: fill its Answer with what changed and the
  fail-towards-refusal limits above, status `resolved`.

## Files

- `crates/binsql-core/src/sql.rs`: `classify` routes `EXPLAIN`/`DESCRIBE`/`DESC`
  to the new `explained`; new `Lexeme`/`lexemes` built on `scan`; doc comments
  on `Kind::Read` and `classify`; new unit tests. `words`, `has_keyword` and
  `scan` are unchanged.
- `crates/binsql/tests/command_mode.rs`: one statement added to each of the two
  existing refusal tests.
- `docs/maps/command-mode/22-explain-analyze-guard.md`: the Answer.

## Verification

```sh
cargo test -p binsql-core sql::
cargo test -p binsql --test command_mode
cargo test --workspace
cargo build --release
```

Then, by hand against a throwaway SQLite file, `binsql query "EXPLAIN ANALYZE
DELETE FROM t"` exits 2 with the write refusal instead of SQLite's syntax
error, and `binsql query "EXPLAIN QUERY PLAN SELECT * FROM t"` still prints
the plan with exit 0.
