---
title: "`--error-format json` gives one structured error record on stderr"
kind: task
mode: afk
status: resolved
blocked_by: []
claimed_by:
---

## Question

Build what 10 settled for this step; the contract is in 10's answer
(`10-structured-errors.md`).

- `Failure` (`cli/mod.rs:25`) gains `category` and `phase`, plus the optional fields `reason`, `code`, `hint`, `detail`, `statement`, `completed` and `transaction`. `usage()` and `failed()` keep their signatures, and builder methods set the extras at each place a failure is raised.
- Add a helper that maps `binsql_core::Error` by variant, never by message text.
- `cli::main` checks the raw arguments for `--error-format` (`--`-aware), falls back to `BINSQL_ERROR_FORMAT`, and prints a bad value as text with exit 2. Add `error-format` to `SHARED_VALUES`.
- The JSON writer prints one compact line with `schema:1` and `exit`, applies the redaction above, and puts the `statement` index in place of the SQL summaries. `note()` prints `type:"notice"` records in JSON mode.
- README gets a "Structured errors" section: the switch, the variable, the schema, the category and phase lists, the revision policy, the redaction rules, and that server messages may contain data values. Update `HELP` too.
- Integration tests (`tests/command_mode.rs`), each checking that stdout is empty on failure:
  - an unknown option → exit 2 and one JSON line with category `usage`, phase `args`;
  - a missing SQLite table → exit 1, `database`/`execute`;
  - `query "delete from t"` → exit 2, `refused`;
  - `--dsn postgres://u:hunter2@127.0.0.1:1/x` → `connect`, and "hunter2" appears nowhere on stderr;
  - the environment variable on its own works;
  - a bad `--error-format` value → text, exit 2;
  - a MySQL DDL dry run's notice is unchecked, so cover notices with a unit test of the writer;
  - default text output stays byte-for-byte the same in all of the above.

## Answer

`--error-format json`, or `BINSQL_ERROR_FORMAT=json`, prints a failure as one JSON line on stderr — type, schema 1, exit, category, phase, a redacted message without SQL, and statement/completed when set — and notices as notice records.

Built in docs/plans/2026-10-07-error-format-json.md (crates/binsql/src/cli/mod.rs: Category, Phase, the Failure builders, error_record, redact and the pre-scan; crates/binsql/src/cli/source.rs; tests/command_mode.rs; README's Structured errors section; HELP's ERRORS). Text mode keeps its bytes and every exit code is unchanged. Review fix: the stored connection string is masked verbatim before the generic patterns rewrite any of it, and a quoted or braced value is masked whole; a read-only or not-plannable refusal no longer quotes the statement in the JSON message; saving, removing or defaulting a source fails in the config phase.

Assumed, not asked: the mask is `****`, the one `mask_dsn` already uses, rather than the contract's `***`.
Assumed, not asked: `reason`, `code`, `hint`, `detail` and `transaction` wait for 41, 42 and 43, which have values for them.
Assumed, not asked: the CLI never sees the resolved connection string, so a resolved secret is caught by the generic patterns, not a verbatim match; those miss `Password = x` with spaces, percent-encoded values and a URL password holding `/`. No message but connect's carries a DSN today.
Assumed, not asked: the pre-scan takes `--error-format` anywhere before the first `--`, so `--arg --error-format` is read as the flag, and a `--` given as a flag's value ends the scan early.
Assumed, not asked: the transaction sentence is left out of JSON with the SQL until 43 gives it a `transaction` field.
Assumed, not asked: `inspect`'s "no table or view named …" is category `database`; `source test` takes `secret` or `connect` from the stage that stopped it.
