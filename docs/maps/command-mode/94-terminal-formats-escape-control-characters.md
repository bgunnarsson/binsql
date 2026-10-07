---
title: "Terminal formats escape control characters in database text"
kind: task
mode: afk
status: resolved
blocked_by: []
claimed_by:
---

## Question

Database-supplied text reaches the terminal with its control characters as
they are. `render.rs`'s table, vertical and raw renderers write `texts()`
unchanged, and stderr messages carry names and error text the same way (25's
answer recorded this as pre-existing). `inspect --definitions` makes it
matter more: its purpose is to print free-form text, comments and literals
as written, in table format on a TTY.

A `.sqlite` file handed to someone can hold a view whose comment carries ESC
or OSC sequences (rewrite earlier lines, plant a hyperlink, set the window
title), or a newline and tabs that read as an extra row under `-o raw`.

Settle and build:

- which formats escape (table, vertical, raw, markdown; not json, jsonl, csv
  or tsv, whose encodings already carry the bytes faithfully);
- the escaped spelling of C0 and C1 controls and DEL, and whether a newline
  inside a cell is escaped or wraps;
- whether stderr messages and notes get the same treatment.

## Context

- Found by the security review of 84; the renderer is the place to fix it,
  not each verb.
- 25's answer lists the stderr paths.

## Answer

`table`, `vertical`, `markdown` and `raw` now spell out the control
characters in values and column names: `\n`, `\r` and `\t` for those three,
`\u{1b}` (lowercase hex, no padding) for every other C0 control, DEL and C1
control. A newline in a cell is escaped, not wrapped. `json`, `jsonl`, `csv`
and `tsv`, and the streaming writer that only serves them, are unchanged.
On stderr the text error, with its context lines, and every note escape the
same characters but keep `\n` and `\t`; the JSON records are unchanged.
Table widths and vertical indents are measured on the escaped text, so the
borders still line up.

Built in docs/plans/2026-10-07-terminal-control-characters.md
(cli/render.rs, cli/mod.rs, README.md). Unit tests put ESC, BEL, CR,
newline, tab, DEL and U+009B in a value and a column name, check the four
reading formats escape them and print no control character but line ends,
check json, jsonl and csv keep them, check table borders stay aligned, and
check a text error keeps its line breaks and escapes ESC and CR.
Live PostgreSQL, MySQL and SQL Server unchecked: Docker is not running here.

The correctness and security reviews found nothing. Both traced every
stdout and stderr write in command mode; the security review noted that a
kept newline lets a server error start a line that reads like binsql's own,
which the plan accepts.

Assumed, not asked:
- `table`, `vertical`, `markdown` and `raw` escape, in values and column names; `json`, `jsonl`, `csv` and `tsv` keep the bytes, and `raw` escapes because a newline or tab breaks its one row per line shape.
- The spelling is `\n`, `\r`, `\t`, and `\u{1b}` for every other C0, DEL and C1 control; a cell is one line.
- A backslash is not doubled, so `C:\temp` stays readable and a stored `\n` reads the same as a newline; the faithful formats tell them apart.
- On stderr the text error and notes keep `\n` and `\t`; the JSON records do not change.
- Only C0, DEL and C1 are escaped; bidi overrides, zero-width characters and the TUI are left alone.
- The escaping is one function in `render.rs`, applied where text is written.
