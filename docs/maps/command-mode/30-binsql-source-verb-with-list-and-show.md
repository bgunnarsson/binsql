---
title: "`binsql source` verb with `list` and `show`"
kind: task
mode: afk
status: open
blocked_by: [18]
claimed_by:
---

## Question

Build what 08 settled for this step; the contract is in 08's answer
(`08-data-source-commands.md`).

- Add `source` to the reserved names. Dispatch in `main.rs` only when a non-flag argument follows, and route to a new `cli/source.rs` with its own `Args::parse` flag lists.
- `list` and `show` build the row described above through the renderer, and never resolve secrets. `shadowed` follows 05's decision 5, with the note on stderr.
- Tests:
  - Bare `binsql source` with no saved match exits 1 with today's message.
  - `binsql source -d postgres` and `binsql source --help` take the TUI path.
  - A saved `source`, top-level or a unique folder leaf, still opens.
  - `mask_dsn` output for each driver's literal form.
  - References print verbatim.
  - JSON and table output for `list`.
  - An unknown op is exit 2.
- Add a SOURCE section to HELP in `cli/mod.rs` and a README section.

## Answer
