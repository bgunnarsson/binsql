---
title: "Render encodes one record at a time without changing output"
kind: task
mode: afk
status: open
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
