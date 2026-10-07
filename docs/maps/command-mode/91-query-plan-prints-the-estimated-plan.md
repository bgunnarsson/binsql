---
title: "`query --plan` prints the estimated plan"
kind: task
mode: afk
status: resolved
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

`query --plan` prints the estimated plan of one read through `-o`, and exits 2 for `--arg`, a statement that cannot be planned, and a write even with `--allow-write`.

Built by docs/plans/2026-10-07-query-plan.md: crates/binsql/src/cli/query.rs checks before calling `Session::plan`, so a refusal is a usage error that names the switch; tests in crates/binsql/tests/command_mode.rs (`query_plan_prints_the_estimated_plan_without_running_it`); the help and README document each backend's form, `-o raw`, the SHOWPLAN grant and that it is an estimate. `--stream` does not exist yet: 72 adds its refusal. Review: four focuses, no findings.

Assumed, not asked: `--plan` with `--arg` is refused outright rather than planning a statement with placeholders — 23 settled that, and planning a parameterised statement differs per backend.
