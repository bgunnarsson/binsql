---
title: "Render encodes one record at a time"
date: 2026-10-07
status: done
---

## Context

Ticket [71](../maps/command-mode/71-render-encodes-one-record-at-a-time-without-changi.md)
prepares `render.rs` for 72's `query --stream`, which writes each record as
its row arrives and so cannot hand `rows()` a whole `ResultSet`. Outcome: the
CSV/TSV header line, the CSV/TSV row line and the JSONL row line each come
from a function that takes one record, and the buffered formats are built
from them, printing the same bytes as before.

Settled: unattended loop; work stays on `main`, each task committed and pushed;
no AI mentions in commits. No change to any output.

## Assumed, not asked

- Three functions: `header_line(columns, delimiter)`, `separated_line(row,
  delimiter)` and `jsonl_line(columns, row)`, each returning the line with its
  newline, so a writer only appends.
- The rule that a header prints only when columns are known stays with the
  caller, as 13 settled; `header_line` does not check it.
- `row_objects` is built from the same per-row object `jsonl_line` encodes, so
  `-o json` and `exec`'s report keep sharing the key-suffixing rule.

## Relevant lore

None in `docs/solutions`.

## Acceptance criteria

- Every existing render test passes unchanged.
- A new test: concatenating the record encoders equals `rows()` for jsonl,
  csv and tsv, with and without a header, over a result with a duplicate
  column name, NULLs, a quoted field and an embedded newline.
- `cargo test --workspace` passes; clippy is clean.

## Tasks

- [x] **1. The encoders.** `cli/render.rs`: `header_line`, `separated_line`,
  `jsonl_line`, `row_object`; `separated`, `jsonl` and `row_objects` on top of
  them; the identity test.
  Verify: `cargo test --workspace`.
