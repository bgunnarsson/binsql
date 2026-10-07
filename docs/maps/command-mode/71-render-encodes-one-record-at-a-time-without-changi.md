---
title: "Render encodes one record at a time without changing output"
kind: task
mode: afk
status: resolved
blocked_by: []
claimed_by:
---

## Question

Build what 13 settled for this step; the contract is in 13's answer
(`13-streaming-contract.md`).

- In `cli/render.rs`, expose record-level encoders: CSV/TSV header line and row line (from `separated`, `render.rs:275-299`), and one JSONL row object (from `row_objects`/`key`, `render.rs:339-371`).
- Rewrite `jsonl` and `separated` on top of them so the buffered output is byte-identical.
- Tests: all existing render tests are unchanged. A new test asserts that concatenating the record encoders equals `rows()` for jsonl, csv and tsv, with and without a header, including duplicate column names, NULLs, quoting and embedded newlines.

## Answer

`cli/render.rs` has three record encoders, each returning one line with
its newline: `header_line` and `separated_line` for CSV and TSV, and
`jsonl_line` for one JSONL object. `separated`, `jsonl` and `row_objects`
are built on them, so `-o json`, `-o jsonl` and `exec`'s report keep one
rule for duplicate column names. The buffered output is byte for byte
what it was: `cmp` on the CLI's jsonl, csv and tsv, with and without
`--no-header`, before and after.

Built in docs/plans/2026-10-07-record-encoders.md (cli/render.rs). The
correctness and security reviews found nothing. The security review noted
that a value starting with `=`, `+`, `-` or `@` is not neutralised against
spreadsheet formula injection; that was so before this change and is left
as it is, since CSV here is data for scripts rather than for a spreadsheet.

Assumed, not asked:
- Each encoder returns its line with the newline, so a writer only appends.
- Whether a header is written when no columns are known stays the caller's call, as 13 settled.
- `row_objects` and `jsonl_line` share one private `row_object`.
