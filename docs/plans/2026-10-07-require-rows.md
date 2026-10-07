---
title: "query --require-rows fails a query that returns no rows"
date: 2026-10-07
status: active
---

## Context

Ticket [15](../maps/command-mode/15-require-rows.md) adds a narrow opt-in
assertion to `query`. Today an empty result is a success, so a script that
checks a condition with a predicate `SELECT` has to parse the output to tell
"no rows" from "some". Outcome: `--require-rows` prints the result as usual and
then exits 1 with an assertion failure on stderr when there were no rows.

Settled: unattended loop; work stays on `main`, each task committed and pushed;
no AI mentions in commits. Additions only: without the flag, output and exit
codes stay as they are.

## Assumed, not asked

- The failure is category `assertion`, a new category, phase `output`: the
  query succeeded and the result has been written; only its emptiness fails.
- The message is "the query returned no rows (--require-rows)".
- `--require-rows` with `--plan` is a usage error: a plan always has rows, so
  the assertion could never fail.
- A statement with no result set (a write under `--allow-write`) has no rows,
  so it fails the assertion, as an empty `SELECT` does.
- `--limit` caps what is fetched, not what counts: one row fetched is enough.

## Relevant lore

None in `docs/solutions`. From the map: tests never reach `ratatui::init`;
never name a zsh variable `status`; every new field or category is in HELP and
the README.

## Acceptance criteria

- An empty result with `--require-rows` prints on stdout exactly what it
  prints without the flag, then `error: the query returned no rows (--require-rows)` on
  stderr, exit 1; in JSON, category `assertion`, phase `output`.
- One or more rows with the flag is exit 0 with unchanged output.
- Without the flag an empty result is exit 0, as before.
- `-o none` with the flag prints nothing on stdout and still asserts.
- A usage or database failure keeps its own exit and category; a refused write
  is still refused.

## Tasks

- [x] **1. The flag.** `cli/query.rs`: `require-rows` joins `SWITCHES`; it
  is refused with `--plan`; after printing, an empty `rows` is a failure.
  `cli/mod.rs`: `Category::Assertion`.
  Verify: `cargo test -p binsql` — integration tests for empty and nonempty,
  `-o none`, JSON, a database failure and the default.

- [ ] **2. HELP and the README.** The flag in QUERY and in the README's query
  section, with a predicate `SELECT` example and the `COUNT(*)` warning; the
  category in Structured errors.
  Verify: `cargo test -p binsql`.
