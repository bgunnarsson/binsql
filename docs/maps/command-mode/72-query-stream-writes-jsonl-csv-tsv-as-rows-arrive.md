---
title: "`query --stream` writes jsonl/csv/tsv as rows arrive"
kind: task
mode: afk
status: resolved
blocked_by: [70, 71]
claimed_by:
---

## Question

Build what 13 settled for this step; the contract is in 13's answer
(`13-streaming-contract.md`).

- `cli/query.rs`: the `stream` switch. Exit 2 for a format other than jsonl/csv/tsv, and exit 2 for a mutating statement even with `--allow-write`. Both checks happen before anything is sent.
- The writer task: channel of 256 items, `BufWriter` of 64 KiB, flush when the channel is empty, whole records only. On a broken pipe it cancels the token and the run exits 0.
- An error after partial output flushes the complete records and exits 1. ⌃C exits 1. `truncated` prints the stderr note through `note()`.
- Update README.md (the flag, its three formats, the `--allow-write` refusal, "only exit 0 means complete", the truncation note on stderr, and that the limit bounds rows held, not server work or row size) and the QUERY section of `HELP` (`cli/mod.rs:106-110`). Remove the Status line for this gap if there is one.
- Integration tests in `crates/binsql/tests/command_mode.rs` against SQLite:
  - Byte identity with the buffered output for jsonl/csv/tsv, with and without `--limit` and `--no-header`.
  - Zero rows prints nothing in either path.
  - `--stream -o json` and `--stream` with the default table format exit 2.
  - `--stream --allow-write` on an `INSERT … RETURNING` exits 2 and writes nothing.
  - `--limit 2` of 5 rows gives 2 records, the stderr note and exit 0, and no note at exactly 5 rows.
  - An error mid-stream (for example a recursive CTE whose fourth row computes `abs(-9223372036854775808)`) gives exit 1 with whole records for rows 1–3 on stdout and `error:` on stderr.
  - A large recursive CTE piped into a reader that closes after one line exits 0 promptly.

## Answer

`binsql query --stream` writes `-o jsonl`, `-o csv` or `-o tsv` a record per
row as rows arrive, through `Session::stream_bound` and a 256-row channel into
a blocking writer on a 64 KiB buffer that flushes whenever the channel runs
dry. Any other format, `--plan`, and a write even under `--allow-write` are
refused before anything runs. The records are the bytes the buffered query
prints; a failure part-way leaves whole records and exits 1; `--limit`
cut-short says so on stderr; a reader that goes away stops the query and
exits 0.

Built in docs/plans/2026-10-07-query-stream.md (cli/stream.rs, cli/query.rs,
cli/mod.rs, tests/command_mode.rs, README.md). Eight tests cover byte identity
for the three formats, the format and write refusals, the limit note, a
failure on the fourth row, a reader that closes, a reader that stalls past
`--timeout-ms`, and `--require-rows`. Live PostgreSQL, MySQL and SQL Server
unchecked: Docker is not running here.

The security review found that a reader which stalls with the pipe open held
binsql past its deadline and past ⌃C, since nothing interrupts a blocked write
to stdout. Fixed: once the command's token is cancelled, the writer gets the
2 s grace to drain and is then left to end with the process. The correctness
review found that a broken pipe returned before the stream's outcome was
looked at, so a query error, ⌃C or timeout followed by EPIPE exited 0, and
that `--require-rows` failed a query with rows when the reader closed before
any were written. Fixed: a closed pipe excuses only the writer's own cancel,
and leaves the row count unknown, so `--require-rows` is not checked.

Assumed, not asked:
- The writer lives in `cli/stream.rs` and cancels a child of the command's token, so a broken pipe never reads as a ⌃C or a deadline.
- `--stream` with `--plan`, and `--stream -o none`, are exit 2.
- A failed write other than a broken pipe exits 1 as `writing output: …`, category `io`.
- The limit note has no `note:` prefix, like the other notes.
- `--require-rows` is skipped when the reader went away, since the count is unknown.
- A stalled reader at the deadline or a ⌃C costs the last record, possibly cut short, after a 2 s grace.
