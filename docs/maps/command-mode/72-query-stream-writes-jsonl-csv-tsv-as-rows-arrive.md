---
title: "`query --stream` writes jsonl/csv/tsv as rows arrive"
kind: task
mode: afk
status: open
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
