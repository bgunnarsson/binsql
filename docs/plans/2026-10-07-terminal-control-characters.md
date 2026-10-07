---
title: "Terminal formats escape control characters in database text"
date: 2026-10-07
status: active
---

## Context

Ticket [94](../maps/command-mode/94-terminal-formats-escape-control-characters.md):
database text reaches the terminal with its control characters as they are,
so a value, a column name or an error message can move the cursor, set the
window title, plant a hyperlink or forge a row. Outcome: the formats a
person reads show every control character as a visible escape, and stderr
does the same for everything but the line breaks it is made of.

## Assumed, not asked

- `table`, `vertical`, `markdown` and `raw` escape, in values and in column
  names. `json`, `jsonl`, `csv` and `tsv` do not: their encodings already
  carry the bytes faithfully, and they are what a script reads. `raw`
  escapes too, because a newline or tab inside a value breaks its one row
  per line, tab-separated shape; a value wanted byte for byte is what `csv`,
  `tsv` and `json` are for.
- The spelling is `\n`, `\r` and `\t` for those three, and `\u{1b}` (hex,
  lowercase, no padding) for every other C0 control, DEL and C1 control. A
  newline in a cell is escaped, not wrapped: a cell is one line.
- A backslash is not doubled, so `C:\temp` stays readable; a value that
  already holds the two characters `\n` reads the same as one holding a
  newline. The faithful formats tell them apart.
- On stderr, the text error and every note escape the same characters
  except `\n` and `\t`, which messages are made of and which cannot move
  the cursor back. The JSON error and notice records are already escaped by
  their encoding and do not change.
- Only C0, DEL and C1 are escaped. Bidirectional overrides and zero-width
  characters are left alone; the TUI draws through ratatui and is not part
  of this.
- The escaping is one function in `render.rs`, applied where text is
  written, rather than in each verb.

## Relevant lore

- 25's answer lists the stderr paths: `failed(error.to_string())`, the
  column-error message and the no-columns note. All of them leave through
  `report` or `note` in `cli/mod.rs`.

## Acceptance criteria

- A value holding ESC, BEL, CR, a newline, a tab, DEL or U+009B prints as
  its escape in `table`, `vertical`, `markdown` and `raw`, and as itself in
  `json`, `jsonl`, `csv` and `tsv`.
- A column name holding a control character is escaped in the same formats.
- Table widths are measured on the escaped text, so the borders line up.
- A text error or note carrying ESC prints `\u{1b}` and keeps its line
  breaks.
- The README says which formats escape and how.

## Tasks

- [x] **1. Escape control characters in the terminal formats and on stderr.**
  A `printable` function in `cli/render.rs` used by `table`, `vertical`,
  `markdown` and `raw` for values and names, and by `report` and `note` in
  `cli/mod.rs` keeping line breaks. Unit tests in both files.
  Verify: `cargo test --workspace -q`, `cargo clippy --workspace --all-targets -q`.
- [x] **2. README.** A line under the formats saying what is escaped.
  Verify: read it beside the `--format` row.
