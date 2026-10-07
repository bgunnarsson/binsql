---
title: "query --stream"
date: 2026-10-07
status: active
---

## Context

Ticket [72](../maps/command-mode/72-query-stream-writes-jsonl-csv-tsv-as-rows-arrive.md)
puts 13's streaming contract on the command line, on top of 70's
`Session::stream_bound` and 71's record encoders. Today `query` holds every
row before printing the first. Outcome: `query --stream -o jsonl|csv|tsv`
writes each record as its row arrives, through a channel of 256 items, and a
run that exits 0 prints exactly the bytes the buffered path prints.

Settled: unattended loop; work stays on `main`, each task committed and pushed;
no AI mentions in commits. Additions only: without `--stream`, output and exit
codes stay as they are.

## Assumed, not asked

- The writer lives in a new `cli/stream.rs`, which `query.rs` calls once the
  checks it already makes have passed.
- The writer cancels a child of the command's token, not the token itself, so
  a broken pipe is never read as a ⌃C or a deadline by `Stop`.
- `--stream` with `--plan` is exit 2: a plan is one document, not rows.
  `--require-rows` works with it, checked against the rows sent once the
  stream has ended, as it is checked after printing today.
- `--stream -o none` is exit 2 along with every other format but the three.
- A write to stdout that fails for any reason but a broken pipe cancels the
  run and exits 1 as `writing output: …`, category `io`, as `print` does.
- The note is `stopped at --limit N; more rows were available`, without a
  `note:` prefix, since `note()` adds none for the other notes either; JSON
  errors carry it as a notice record.
- `--timeout-ms` covers the stream as it covers the buffered run.

## Relevant lore

None in `docs/solutions`. From the map: tests never reach `ratatui::init`;
never name a zsh variable `status`; every new flag is in HELP and the README;
the CLI takes `--driver sqlite` with a plain path for a DSN.

## Acceptance criteria

- On SQLite, `--stream` prints byte for byte what the buffered path prints for
  jsonl, csv and tsv, with and without `--limit` and `--no-header`; zero rows
  prints nothing in either.
- `--stream -o json`, `--stream` with the default format, and `--stream
  --plan` exit 2; `--stream --allow-write` on `INSERT … RETURNING` exits 2 and
  writes nothing.
- `--limit 2` of 5 rows gives 2 records, the stderr note and exit 0; `--limit
  5` gives 5 and no note.
- An error on the fourth row exits 1 with rows 1–3 whole on stdout and
  `error:` on stderr.
- An endless recursive CTE piped into a reader that closes after one line exits
  0 promptly.
- `cargo test --workspace` passes; clippy is clean. PostgreSQL, MySQL and SQL
  Server unchecked: Docker is not running here.

## Tasks

- [ ] **1. The switch and the writer.** `cli/stream.rs`: the writer on
  `spawn_blocking`, 64 KiB `BufWriter`, flushed when the channel is empty.
  `cli/query.rs`: `stream` in `SWITCHES`, the format and write refusals before
  anything is sent, the note, `--require-rows`.
  Verify: `cargo test --workspace` — the integration tests above.

- [ ] **2. HELP and the README.** `--stream` under QUERY; in the README, the
  three formats, the `--allow-write` refusal, that only exit 0 means complete,
  the note on stderr, and that the limit bounds rows held, not server work or
  row size.
  Verify: `cargo test -p binsql`.
