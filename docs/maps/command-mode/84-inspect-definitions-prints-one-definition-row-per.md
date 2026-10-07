---
title: "`inspect --definitions` prints one definition row per selected object"
kind: task
mode: afk
status: resolved
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

`binsql inspect --definitions` prints one row per selected object:
`catalog, schema, object, kind, form, definition`. Objects are selected and
ordered as `--columns` does, through one shared helper, so a name is matched
exactly and an ambiguous one is exit 2. `form` is never null; `definition` is
null when binsql has no text. Rows marked `unsupported` or `withheld` are
counted in one stderr note (`3 definitions not given: 2 unsupported, 1
withheld`), silenced by `-o none`. Every definition is read before anything
prints, so a failure leaves stdout empty and reads `definition of <object>:
<error>`. `--definitions --columns` exits 2 before connecting.

Built in docs/plans/2026-10-07-inspect-definitions.md (cli/inspect.rs,
cli/mod.rs, README.md, tests/command_mode.rs). Unit tests cover the sort, a
null definition and the stderr note's wording; command tests cover JSON,
JSONL and CSV against SQLite, one object by exact name and its case-only near
miss, and the `--columns` refusal. `inspect`, `inspect <t>` and `--columns`
print what they did before. Live PostgreSQL, MySQL and SQL Server unchecked:
Docker is not running here.

The correctness review found nothing. The security review found, at low
severity, that table, vertical and raw output write control characters in
database text as they are; that predates this ticket (25 recorded it) but
`--definitions` prints free-form text, so it is now ticket 94 rather than a
note.

Assumed, not asked:
- Selection is `columns_of`'s, shared by both verbs through one helper.
- The stderr note names only the forms that occur and is left out when every row has text.
- An acquisition failure reads `definition of <object>: <error>`.
- The README's MySQL caveat is left for 82, since MySQL is `unsupported` until then.
- `--definitions --columns` is refused before connecting.

Live, 2026-10-07, on `eimskip/local` (Azure SQL 12.0.2000.8, read-only source):

- `inspect --definitions -o csv` exits 0 in about 5 s. It prints the header and 103 table rows, each `form` `unsupported` with an empty `definition`, and stderr reads `103 definitions not given: 103 unsupported`.
- `inspect --definitions -o json __EFMigrationsHistory` gives one row with `definition: null`.
- A system view (`sys.database_firewall_rules`) is not selected: "no table or view named …".

Neither `eimskip/local` nor `osar/local` has a user view, so a view's `create` text and a view with null text (`WITH ENCRYPTION`) remain unchecked live.
