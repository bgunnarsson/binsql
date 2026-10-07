---
title: How does opt-in result streaming report partial output?
kind: grilling
mode: hitl
status: resolved
blocked_by: [3]
claimed_by:
---

## Question

Decide an opt-in query streaming contract for large exports: which formats,
flag, metadata channel and termination semantics. Recommend bounded buffering
for JSONL/CSV/TSV first, leaving existing buffered output unchanged. Preserve
row-only JSONL, existing JSON envelopes and limit defaults (absent/zero means
unlimited); do not append metadata rows to existing streams. Explain truncation,
errors after partial output, cancellation, broken pipe and success detection.
Settle interaction with query's existing --allow-write explicitly. A fetch cap
bounds retained rows but does not promise bounded server work or row byte size. Cut adapter streaming
and CLI writing into separate implementation tasks after this decision.

## Context

Ticket 03; `crates/binsql/src/cli/query.rs:61`, cli/render.rs:329,
cli/mod.rs:338; `crates/binsql-core/src/adapter/mod.rs`, adapter/sqlx_common.rs:104,
adapter/mssql.rs:158 and value.rs. See psql FETCH_COUNT and mysql --quick docs.
No changes to exec receipts or query safety permissions.

## Answer

`binsql query` gets one opt-in switch, `--stream`. It is allowed only with `-o jsonl|csv|tsv` and only for a statement that does not write, even when `--allow-write` is given. It writes each row to stdout as the row arrives, through a bounded queue. A run that finishes without error prints exactly the bytes the buffered path prints for the same rows. Exit status stays the only signal that the output is complete. Truncation goes to stderr as a note, never into the stream.

### Settled contract

**Today**
- `query` collects every row into a `ResultSet` (`crates/binsql/src/cli/query.rs:60-63`). It renders the whole result into one `String` (`cli/render.rs:70-82`), and JSONL first builds a `Vec<Json>` (`cli/render.rs:329-335`, `339-353`). It then writes that string once (`cli/mod.rs:338-348`).
- Both adapters already read rows one at a time from a driver stream (`binsql-core/src/adapter/sqlx_common.rs:133-177`, `adapter/mssql.rs:169-218`). They keep every row in a `Vec`.
- `--limit` arrives as `Some(n)`, or `None` when it is absent or 0 (`query.rs:21-28`). When the adapter sees a row beyond `n`, it sets `truncated = true` and stops reading (`sqlx_common.rs:166-169`, `mssql.rs:211-214`). So `truncated` means "at least one more row existed", not "the count reached n".
- Only `-o json` reports `truncated` (`render.rs:320`). The table and vertical footers say "more available" (`render.rs:434-436`). JSONL, CSV and TSV print rows only (`render.rs:275-299`, `329-335`).
- The sqlx adapters learn the columns only when the first row arrives (`sqlx_common.rs:159-165`). The SQL Server adapter learns them from Metadata, before any row (`mssql.rs:194-201`).
- A broken pipe already counts as success (`cli/mod.rs:345`). Any other failure exits 1 (`cli/mod.rs:70-81`, `query.rs:63`).

**The new flag**
- `query --stream` is a switch added to `SWITCHES` (`query.rs:16`). Without it, every byte of today's output, every exit code and every buffered path stays the same.
- Formats: `jsonl`/`ndjson`, `csv` and `tsv` only. Any other `-o`, including the default `table`, exits 2 with a usage message that names the three formats. Assumed, not asked: no streamed table, vertical, markdown, raw or JSON — table and vertical need every row before they can size columns or print the footer, and a streamed JSON envelope would break `-o json`'s single-document contract.
- `--no-header` works as it does today. `--pretty` has no effect on JSONL, as today (`render.rs:332`).
- **Byte identity:** a streamed run that exits 0 writes exactly the bytes the buffered path writes for the same result. That includes the CSV/TSV header appearing only when columns are known (`render.rs:278`), so a zero-row SQLite result prints nothing in both paths, and SQL Server prints the header in both.
- **No metadata in the stream:** no trailer row, no `truncated` record, and the JSONL output stays rows only.

**`--allow-write`**
- `--stream` refuses any statement where `statement.kind.mutates()` (`query.rs:48`) is true, whether or not `--allow-write` is given. It exits 2 before connecting… more precisely, after splitting the statement and before sending anything, at the same point as today's refusal (`query.rs:34-54`). The message points to dropping `--stream` or using `exec`.
- Assumed, not asked: refuse rather than allow. If the pipe breaks or ⌃C arrives part way through `INSERT … RETURNING`, nobody can tell whether the write committed (unchecked per backend). The refusal adds no write permission, and lifting it later would be purely additive.
- The data source's read-only guard still applies through `Session` (`session.rs:141`).

**`--limit`**
- Same parsing and meaning: absent or 0 means unlimited (`query.rs:21-28`), and there is still no default cap.
- The adapter stops reading at the first row beyond the limit, as today. "A fetch cap bounds retained rows; it does not promise bounded server work or row byte size" — the SQL is sent unchanged.
- When `truncated` is true, `--stream` prints one line to stderr through `note()` (`cli/mod.rs:352`): `note: stopped at --limit N; more rows were available`. The exit is still 0. Assumed, not asked: stderr rather than a new channel — this follows MAP's rule that notes go to stderr, and ticket 10 may later give the note a structured form.

**Bounded buffering and backpressure**
- The adapter sends each item (`Columns`, then `Row`s) into a bounded `tokio::sync::mpsc` channel holding 256 items. Each send also listens for cancellation in the same `tokio::select!`, so a full channel slows the reader, and through it the driver.
- One writer, on `spawn_blocking`, encodes each record into a `BufWriter<Stdout>` of 64 KiB. It flushes whenever the channel is momentarily empty, so the first rows of a slow query show up early.
- Memory is bounded in rows (256 items plus the writer buffer), not in bytes: one very large value is still held whole.
- Assumed, not asked: 256 and 64 KiB are internal constants, with no flag.

**What a caller can rely on**
- Success: exit 0 and only exit 0 means the stream is complete, or deliberately cut short by `--limit`.
- Error after partial output, such as a server error on row 1,000: the records already written stay on stdout. The writer writes and flushes whole records only, so stdout always ends at a record boundary. The error goes to stderr as `error: …` and the exit is 1. Callers must discard stdout unless the exit is 0, which the README must say.
- ⌃C: `cancel_on_interrupt` (`cli/mod.rs:243`) fires the same token as today. The adapter stops the server the way `run` does (`adapter/mod.rs:42-47`), the records written so far stay, and the exit is 1, as cancellation exits today. Unchecked: where the SQL Server adapter turns `collect`'s `Ok(None)` (`mssql.rs:220-222`) into `Error::Cancelled` — the task must confirm this.
- Broken pipe (`| head`): the writer records it, drops its receiver and cancels the token. The query stops by the cancel path, and the CLI maps "cancelled after a broken pipe" to exit 0, consistent with `print` (`cli/mod.rs:345`). Unchecked: whether PostgreSQL, MySQL and SQL Server stop work on the server promptly. The cancel path's own comments promise this (`adapter/mod.rs:42-47`), but nobody has measured it.
- `exec`, `inspect`, exec receipts and permissions are unchanged.

### Build tickets

**70 — Core streams rows into a bounded sink**
blocked_by: []
- `binsql-core/src/value.rs` (or a new `stream.rs`): `enum Streamed { Columns(Vec<Column>), Row(Vec<Value>) }`, and `StreamSummary { rows: usize, rows_affected: Option<u64>, truncated: bool, elapsed }`.
- `Adapter::stream(&self, statement, limit, sink: mpsc::Sender<Streamed>, cancel) -> Result<StreamSummary>`. The default method calls `run` and forwards, so the addition is purely additive.
- Give the drain loop a sink parameter in `sqlx_common.rs:101-187` and `mssql.rs:158-231`: `run` collects into a `Vec` exactly as today, and `stream` sends each item through `select!` with `cancel`.
- `Columns` is sent once, when columns become known. That is the first row for sqlx and Metadata for SQL Server, mirroring today's population.
- A closed receiver behaves like a cancel: the same server-stop path, returning `Error::Cancelled`.
- Add `Session::stream_bound`, which applies `guard_read_only` like `run_bound` (`session.rs:134-146`).
- Tests (SQLite): items arrive in order, and their collected rows equal `run`'s for the same SQL. `limit` stops at N with `truncated` true at N+1 rows and false at exactly N. Cancelling or dropping the receiver mid-stream returns `Cancelled`, and the session then runs another statement. All existing `run` tests pass unchanged.
- Live PostgreSQL, MySQL and SQL Server checks only where the existing harness has them; otherwise mark them unchecked in the PR.

**71 — Render encodes one record at a time without changing output**
blocked_by: []
- In `cli/render.rs`, expose record-level encoders: CSV/TSV header line and row line (from `separated`, `render.rs:275-299`), and one JSONL row object (from `row_objects`/`key`, `render.rs:339-371`).
- Rewrite `jsonl` and `separated` on top of them so the buffered output is byte-identical.
- Tests: all existing render tests are unchanged. A new test asserts that concatenating the record encoders equals `rows()` for jsonl, csv and tsv, with and without a header, including duplicate column names, NULLs, quoting and embedded newlines.

**72 — `query --stream` writes jsonl/csv/tsv as rows arrive**
blocked_by: [70, 71]
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
