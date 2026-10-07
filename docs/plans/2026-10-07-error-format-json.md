---
title: "`--error-format json` prints one structured error record on stderr"
date: 2026-10-07
status: active
---

## Context

Ticket [40](../maps/command-mode/40-error-format-json-gives-one-structured-error-recor.md)
builds the first step of [10](../maps/command-mode/10-structured-errors.md)'s
contract. Today every command-mode failure is a `Failure { message, usage }`
(`crates/binsql/src/cli/mod.rs:25`) printed as `error: …` text by `cli::main`.
Outcome: `--error-format json`, or `BINSQL_ERROR_FORMAT=json`, turns that into
one compact JSON line on stderr, carrying a category and a phase set where the
failure is raised, and `note()` prints notices as JSON records too.

Settled: unattended loop; work stays on `main`, each task committed and pushed;
no AI mentions in commits. Additions only: text output stays byte-for-byte the
same, and no exit code changes.

## Assumed, not asked

- The mask is `****`, the one `binsql_core::config::mask_dsn` already uses,
  rather than the contract's `***`: one mask across binsql.
- `reason`, `code`, `hint`, `detail` and `transaction` are added by 41, 42 and
  43, which have values to put in them. This step adds `statement` and
  `completed`, which it can fill.
- The CLI never sees the resolved connection string (`Session::open` resolves
  it inside the core), so a resolved secret is caught by the generic patterns
  (`user:pass@`, `password=`, `pwd=`, `accesstoken=`, JWTs), not by a verbatim
  match. The stored one is matched verbatim.
- The pre-scan reads `--error-format` wherever it stands before `--`, so
  `--arg --error-format` — a value that happens to be spelled like the flag —
  is taken as the flag. The argument parser would read it the other way.
- In JSON the transaction sentence ("the transaction was rolled back; nothing
  was kept") is left out with the SQL summaries, until 43 can say what the
  outcome was in a `transaction` field.
- `inspect`'s "no table or view named …" is category `database`: the database
  does not have what was asked for.
- `source test`'s failure takes its category from the stage that stopped it:
  `secret` for the secret stage, `connect` otherwise.

## Relevant lore

None in `docs/solutions` (there is none). From the map: tests never reach
`ratatui::init`; a connect failure in a test uses `?sslmode=bogus` so it fails
when the options are parsed rather than at a closed port; never name a zsh
variable `status`.

## Acceptance criteria

- `Failure` gains `category`, `phase`, `statement`, `completed`, `context` and
  the source's connection string for redaction; `usage()` and `failed()` keep
  their signatures and default to `usage`/`args` and `other`/`execute`.
- A `binsql_core::Error` is mapped to a category by its variant, never by its
  text.
- Text mode prints `error: {message}`, then each context line as `\n  {line}`,
  then the help pointer for a usage failure: exactly today's bytes.
- `--error-format json`, `--error-format=json` or `BINSQL_ERROR_FORMAT=json`
  gives one line, `{"type":"error","schema":1,"exit":…,"category":…,"phase":…,"message":…}`
  plus `statement`/`completed` when set, and nothing else on stderr. The flag
  wins over the variable; an empty variable is unset.
- A bad or missing `--error-format` value is a text usage error, exit 2.
- In JSON mode the message is redacted and holds no SQL, and `note()` prints
  `{"type":"notice","schema":1,"message":…}`; `-o none` still hides notes.
- The integration tests ticket 40 lists pass, each with stdout empty.

## Tasks

- [ ] **1. Failure carries a category, a phase and context.**
  In `cli/mod.rs`: `Category` (`usage source config secret connect refused
  database cancelled io other`) and `Phase` (`args input config connect prepare
  execute output`), each with `as_str`; `Failure` gains the fields above and
  builders `.category()`, `.phase()`, `.statement()`, `.completed()`,
  `.context()`. `caused(message, &Error)` and `core(Error)` map a core error.
  Every site gets its category and phase (list in the ticket's design: the
  workspace load is `config`; a missing saved source or default `source`;
  file, stdin and output errors `io`; split, guard and bind `prepare`;
  refusals `refused` with the SQL moved to context; `exec --no-tx` sets
  `statement` and `completed`).
  Verify: `cargo test -p binsql` — every existing test passes unchanged.

- [ ] **2. The format, the record and the redaction.**
  `cli::main` pre-scans for `--error-format`, falls back to
  `BINSQL_ERROR_FORMAT`, keeps the choice in a `OnceLock` for `note()`, and
  writes the record. `error-format` joins `SHARED_VALUES` and `source`'s
  `VALUES`. `binsql_core::session::masked` becomes public so the stored DSN is
  masked the way the core masks it. Unit tests: the pre-scan, each redaction
  pattern, the notice record.
  Verify: `cargo test -p binsql`.

- [ ] **3. Integration tests** in `tests/command_mode.rs`, per the ticket;
  `BINSQL_ERROR_FORMAT` is removed from the test environment.
  Verify: `cargo test -p binsql --test command_mode`.

- [ ] **4. HELP and the README.** A "Structured errors" section: the switch,
  the variable, the record, the categories and phases, the revision policy,
  the redaction, and that a server's message can hold data values.
  Verify: `cargo test -p binsql`.
