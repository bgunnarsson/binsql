---
title: "`--error-format json` gives one structured error record on stderr"
kind: task
mode: afk
status: open
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
