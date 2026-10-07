---
title: What structured stderr contract does a caller opt into?
kind: grilling
mode: hitl
status: resolved
blocked_by: [3]
claimed_by:
---

## Question

Choose the additive error contract: recommend `--error-format text|json`,
text by default, independent of `--format`, with unchanged exits 0/1/2.
Settle the JSON schema and revision policy, stable categories and phases,
optional native code/hints, and whether all notices become JSON records too.
Define behaviour for invalid flags and errors before full argument parsing;
never emit an error on result stdout. Include redaction of DSNs, credentials,
SQL literals and subprocess diagnostics, without removing useful remediation.
Do not infer native codes or retryability from formatted prose. Decide how to
represent unknown transaction outcome rather than claiming rollback succeeded.
This decision should yield a small implementation task, not implement it here.

## Context

Ticket 03, its primary-source links, `crates/binsql/src/cli/mod.rs:24`,
`cli/exec.rs`, `crates/binsql-core/src/error.rs`, and secret resolution.
Read 09's answer if available for authentication categories; otherwise leave
those details for a task blocked by 09. No new exit numbers or changed defaults.

## Answer

Callers who want machine-readable errors add `--error-format json` (or `BINSQL_ERROR_FORMAT=json`); each failure then prints as one JSON line on stderr. Text stays the default, the exit codes stay 0/1/2, and stdout still carries nothing but `--format` output.

### Contract

**Switch.** `--error-format text|json` joins the shared flags (`crates/binsql/src/cli/mod.rs:138`). `BINSQL_ERROR_FORMAT` does the same through the environment, matching `BINSQL_CONN` and the others (`cli/mod.rs:96`), and the flag wins over the variable. It has nothing to do with `-o`: `-o json` alone never changes stderr (03, `03-agent-ergonomics-survey.md:101`). It only works after a verb, like every other flag. `binsql --error-format json query` still goes to the TUI path and fails there (`crates/binsql/src/main.rs:40`, `:262`). The variable covers callers who can't control where the flag goes.
Assumed, not asked: an environment variable as well as the flag — agents set it once per session, and it reaches failures that happen before the arguments are read.

**Errors before the arguments are fully read.** `cli::main` checks the raw argument list for `--error-format` (both `--error-format json` and `--error-format=json`) before handing it to the verb, stopping at `--`. That way an unknown option or a missing value, which `Args::parse` reports through `usage()` (`cli/args.rs:60`, `:68`, `:74`), still comes out as JSON. If the value of `--error-format` itself is bad or missing, the error is printed as text with exit 2, since no format was settled. Errors from the TUI path in `main.rs` are left as they are.

**The record.** On failure, stderr gets exactly one compact line in JSON mode, and nothing else. The text `error:` line and the `run binsql --help` pointer (`cli/mod.rs:73-77`) are left out. Fields:

```
{"type":"error","schema":1,"exit":1|2,"category":"…","phase":"…","message":"…",
 "reason":?, "code":?, "hint":?, "detail":?, "statement":?, "completed":?, "transaction":?}
```

Absent optional fields are left out, never written as null.

- **`exit`** is the process exit code. It still comes from `Failure.usage` exactly as today (`cli/mod.rs:74-80`), so the category never changes an exit code.
- **`category`**, the v1 list:
  - `usage`: argument or flag mistakes, and `Error::Placeholders`.
  - `source`: no saved source, an ambiguous name, or a missing default (`cli/mod.rs:194-229`).
  - `config`: the Workspace fails to load (`:171`).
  - `secret`: keychain or Key Vault (`Error::Config` raised in `secrets/azure.rs:41-63`).
  - `connect`: `Error::Connect`, and connection failures in general.
  - `refused`: a write sent to `query` (`cli/query.rs:49`), a destructive statement without `--force` (`cli/exec.rs:124`), or `Error::ReadOnly` (`error.rs:23`). The exit code stays 2/2/1 as today.
  - `database`: `Error::Query`, the database said no.
  - `cancelled`: `Error::Cancelled`.
  - `io`: reading the file or stdin, or writing output (`cli/mod.rs:264`, `:332`, `:346`).
  - `other`.
- **`phase`**: `args`, `input`, `config`, `connect` (this includes secret resolution, because `Session::open_with` does both, `session.rs:44-45`), `prepare` (split, guard and bind — nothing has been sent yet), `execute`, `output`.
- **Mapping without guessing.** The category is set where the error is created, or by matching the variant of `binsql_core::Error` (`error.rs:6-44`). It is never taken from the message text.
- **`reason`** is a finer, stable kind. In v1 it is only used for `secret` and the Azure AD token failure (task 41).
- **`code`** is the database's own error code as a string. It is only included when a driver gives it as a typed field (task 42), never parsed out of the message.
- **`hint`** is the next step, kept separate from `message`.
- **`detail`** is a subprocess's stderr after redaction.
- **`statement`** is the 1-based position of the failing statement in the batch.
- **`completed`** is how many statements already ran in a batch without a transaction (`cli/exec.rs:76-82`).
- **`transaction`** is one of:
  - `none`: no transaction was opened.
  - `rolled_back`: the rollback was confirmed.
  - `unknown`: the commit or rollback failed, the connection was lost, or the run was cancelled mid-batch.

**Revision policy.** Version 1 is what `schema` says. Adding a field, a category, a phase or a reason keeps the version at 1, so consumers must treat values they don't know as `other`. Renaming, removing or changing the meaning of a field raises the version. Ticket 14's capability list should include `error_schema: 1`.

**No retry field.** There is no `retryable` field in v1. A timed-out write is not proof that retrying is safe (03, `03-agent-ergonomics-survey.md:165`).

**Notices.** In JSON mode, every `note()` (`cli/mod.rs:352`) becomes `{"type":"notice","schema":1,"message":…}`, one per line on stderr. The rule that hides notes under `-o none` (`:353`) stays as it is.
Assumed, not asked: notices are only structured in JSON error mode, and `-o none` still hides them — this changes the least.
Unchecked: whether anything in the CLI writes to stderr other than `note()` and `cli::main`. Task 40 has to look for that.

**Redaction (JSON mode).**
- `message` and `detail` are scrubbed before they are printed:
  - the DSN as stored and as resolved is masked wherever it appears verbatim;
  - `user:pass@` becomes `user:***@`;
  - values of `password=`, `pwd=` and `accesstoken=` are masked;
  - JWT-shaped tokens (`eyJ…`) are masked.
- Use the core's existing masking helper if there is one. MAP 07 says the core holds masking, but I haven't found the function, so this is unchecked.
- binsql adds no SQL text in JSON mode: the `statement:` summaries (`cli/exec.rs:85`, `:127`, `cli/query.rs:52`) are replaced by the `statement` index.
- The database's own message is passed through as it is, because it is the fix the caller needs. The README must say it can contain data values.
- Key Vault references (`keyvault://vault/secret`) are not secret and stay in.

Assumed, not asked: server messages are not scrubbed of literal values — removing them would take out the remediation, and binsql cannot reliably tell a value from an identifier.

**Transaction outcome.** Today `cli/exec.rs:62-65` adds "the transaction was rolled back; nothing was kept" to every error from `run_transaction`. That includes `Error::ReadOnly`, which is raised before anything is sent (`session.rs:162-164`), and failures during commit. The core has to report what actually happened (task 43); the CLI must not assume it. In text mode, the "rolled back" sentence only appears when `rolled_back` is confirmed. Otherwise it says the outcome is unknown, or that nothing was sent. This corrects a false claim; it is not a change to an output format.
Unchecked: how each adapter's `run_transaction` handles a failed rollback or commit.

**Text mode** stays byte-for-byte the same, apart from task 43's correction.

### Build tickets

**40 — `--error-format json` gives one structured error record on stderr**
blocked_by: []
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

**41 — Key Vault and Azure AD failures carry a reason**
blocked_by: [40]
- `secrets/azure.rs` classifies `az`'s raw stderr where it already reads it (`explain()`, `:70-98`) into a typed reason: `az-missing`, `az-unauthenticated`, `vault-forbidden`, `secret-not-found`, `vault-not-found` (09). It returns a new `Error::Secret { reference, reason, hint, detail }`, whose text output is identical to today's message.
- The `fedauth=` token failure (`adapter/mssql.rs:705`, per 09) gets the reason `azure-ad-token` under category `connect`.
- The CLI maps these to category `secret` (or `connect`) with `reason`, `hint`, and a redacted, length-capped `detail`. If no substring matches, there is no `reason` and the stderr goes into `detail`.
- Tests: unit tests for each classification using the existing stderr fixtures (`azure.rs:104-130`); a test that a JWT in `detail` is masked; a test that text output is unchanged.

**42 — Database errors carry the native code when the driver types it**
blocked_by: [40]
- Add a core `Error::native_code()` that walks the anyhow chain of `Error::Query` and `Error::Connect`, reading sqlx `DatabaseError::code()` (PostgreSQL, MySQL, SQLite) and tiberius's server error number (SQL Server, as a decimal string). Return `None` otherwise.
- Unchecked: that the adapters keep the driver error in the chain, and what the MySQL `code()` returns. The README lists the code's meaning per backend.
- Tests: a SQLite UNIQUE violation gives a `code` that matches the value read from the driver in the test; a binsql-side failure (`refused`) has no `code`.

**43 — exec reports the actual transaction outcome**
blocked_by: [40]
- `run_transaction` (core `session.rs:154`, the adapters) reports whether the rollback was confirmed, whether the commit or rollback failed, and whether the batch was refused before BEGIN. For example, an `Error::Transaction { source, outcome }` whose Display matches today's text.
- `cli/exec.rs:62-65` stops claiming a rollback unless it is `rolled_back`. JSON gets `transaction` (`none` / `rolled_back` / `unknown`) and, for `--no-tx`, `completed` (`:76-82`). Ticket 12's cancellation and timeout reporting should use the same field.
- Tests:
  - a SQLite batch that fails at statement 2 → `rolled_back`, `statement:2`, and the table is unchanged;
  - a read-only source refusing a batch → `transaction:none`, and the text no longer says "rolled back";
  - a two-statement batch with `--no-tx` that fails at statement 2 → `completed:1`;
  - a failed commit is covered by a unit test with a fake adapter if one exists; otherwise it is recorded as unchecked.
