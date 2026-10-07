---
title: "`query --plan` prints the estimated plan"
kind: task
mode: afk
status: open
blocked_by: [90]
claimed_by:
---

## Question

Build what 23 settled for this step (see `23-plan-shape.md`, "Settled shape"):

- A `--plan` switch on `query`, sent to `Session::plan`; the result goes through `render::rows` like any query.
- Exit 2, with nothing sent, for: a statement that is not plannable, a write even with `--allow-write` (the message says to drop `--plan`), `--arg`, and `--stream` if 72 has landed (if not, 72 adds the check).
- Without `--plan`, output is byte-identical to today.
- README's query section and the QUERY part of `HELP` (`cli/mod.rs`) gain `--plan`: the per-backend table, `-o raw` for the bare document, reads only, the SHOWPLAN grant, and that it is an estimate only.
- Tests in `crates/binsql/tests/command_mode.rs` against SQLite:
  - `-o json`, `-o csv` and `-o raw` output;
  - a read-only registered source plans a read;
  - each exit-2 case;
  - `--plan` over a `DELETE` with `--allow-write` exits 2 and the row is still there.

## Context

- 23's answer is the contract; 17's answer has each backend's estimated form and sources.

## Answer
