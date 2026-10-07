---
title: "Terminal formats escape control characters in database text"
kind: task
mode: afk
status: open
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
