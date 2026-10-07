---
title: "`inspect --definitions` prints one definition row per selected object"
kind: task
mode: afk
status: open
blocked_by: [26, 80]
claimed_by:
---

## Question

Build what 16 settled for this step; the contract is in 16's answer
(`16-ddl-output.md`).

- Adds the switch.
- Reuses 25 and 26's selection, sort and exact-name matching.
- Builds the row shape above, the stderr count of `unsupported` and `withheld` rows, all-or-nothing failure, and exit 2 when given with `--columns`.
- Tests, against a SQLite fixture:
  - JSON with the exact envelope and rows, plus JSONL and CSV;
  - an exact name;
  - `--definitions --columns` exits 2 with empty stdout;
  - a unit test of the stderr note for `unsupported` rows;
  - `inspect`, `inspect <t>` and `inspect --columns` print byte-identical output to before.
- README: the inspect section, a per-backend coverage table (rows for 81–83 marked unsupported until they land), the context-not-restore caveat, and the Status line. HELP: `--definitions` in `cli/mod.rs`.

## Answer
