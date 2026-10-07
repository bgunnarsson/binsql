---
title: "exec reports the actual transaction outcome"
date: 2026-10-07
status: active
---

## Context

Ticket [43](../maps/command-mode/43-exec-reports-the-actual-transaction-outcome.md)
builds the transaction step of [10](../maps/command-mode/10-structured-errors.md)'s
contract. Today `cli/exec.rs` adds "the transaction was rolled back; nothing
was kept" to every error `Session::run_transaction` returns, including a
read-only refusal raised before anything is sent and a failed commit, and the
adapters throw away whether their rollback worked. Outcome: the core says what
became of the transaction, text mode only claims a rollback that was
confirmed, and the JSON record carries `transaction` and, inside a
transaction, the failing statement's position.

Settled: unattended loop; work stays on `main`, each task committed and pushed;
no AI mentions in commits. Additions only: text output stays byte-for-byte the
same apart from this correction, and no exit code changes.

## Assumed, not asked

- `Error::Transaction { error, outcome, statement }` wraps only failures after
  the transaction began, and prints exactly the wrapped error's text. Any
  other error from `run_transaction` (the read-only guard, a failed `BEGIN`,
  a connection that could not be had) means no transaction was opened: the
  CLI calls that `none`.
- A failed `BEGIN` is `none`: nothing in the batch ran.
- A statement's failure, a cancel included, is `rolled_back` when the
  adapter's rollback returns success and `unknown` when it fails. On SQL
  Server a cancel replaces the connection, which takes the transaction with
  it unconfirmed, so it is `unknown` there.
- A failed commit, or a failed rollback ending a dry run, is `unknown` with
  no `statement`: the batch itself ran.
- Text for `unknown`: "whether the transaction was committed or rolled back
  is unknown"; for `none`: "no statement was run".
- `transaction` is only set by `exec`'s transactional path; `--no-tx` keeps
  its `completed` and gets no `transaction`.

## Relevant lore

None in `docs/solutions`. From the map: tests never reach `ratatui::init`;
never name a zsh variable `status`; clippy's `result_large_err` holds
`Failure` to 128 bytes.

## Acceptance criteria

- A SQLite batch failing at statement 2 gives `transaction:"rolled_back"`,
  `statement:2`, and the table is unchanged; the text still says "the
  transaction was rolled back; nothing was kept".
- A read-only source refusing a batch gives `transaction:"none"`, and the
  text no longer says "rolled back".
- `--no-tx` failing at statement 2 gives `completed:1`.
- A commit that fails (SQLite's deferred foreign key) gives
  `transaction:"unknown"` and no rollback claim.
- The record writes `transaction` after `completed`.

## Tasks

- [x] **1. The core says how the transaction ended.**
  `error.rs`: `TransactionOutcome` with `as_str`, `Error::Transaction`,
  `native_code` looks through it. `adapter/sqlx_common.rs` and
  `adapter/mssql.rs` wrap statement, commit and rollback failures with the
  outcome they saw.
  Verify: `cargo test -p binsql-core`.

- [x] **2. exec reports it.**
  `cli/mod.rs`: `Transaction` on `Failure`, filled by `caused` from the core
  error, written by `error_record`. `cli/exec.rs` picks the sentence by the
  outcome. Integration tests per the criteria.
  Verify: `cargo test -p binsql`.

- [x] **3. HELP and the README.** The Structured errors section lists
  `transaction` and its values.
  Verify: `cargo test -p binsql`.
