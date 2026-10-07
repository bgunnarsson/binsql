---
title: "`binsql source` verb with `list` and `show`"
kind: task
mode: afk
status: resolved
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

Built: `binsql source list` and `source show NAME` list saved data sources with their DSN masked; plan docs/plans/2026-10-07-source-list-show.md.

- `crates/binsql/src/cli/source.rs` lists every saved data source (name, scope, driver, dsn, readonly, open_on_start, default, description, shadowed); `show NAME` resolves the name as `--conn` does. Credential-store references print verbatim; any other DSN goes through `binsql_core::config::mask_dsn(backend, dsn)`, which reads the string the way that driver's adapter does (libpq keywords, URLs, go-mysql userinfo, ADO quoting, SQL Server's URL-to-ADO rewrite and fedauth split) and over-masks where unsure.
- A saved name equal to a reserved command gets a note on stderr saying how to open it.
- `list NAME`, `show` without one name, and unknown operations exit 2 until tickets 31-34 add the rest.
- Assumed, not asked: unknown operations exit 2 until 31-34; the column set above; the shadow note on `show` as well as `list`; masking mirrors each driver's parser rather than one generic rule — the least that hides every password the drivers accept.
- Assumed, not asked: opening a saved source named `source` through the TUI path is unit-tested only, because integration tests must not reach `ratatui::init` (`/dev/tty`).
- Plausible, not fixed: masking is a mirror of each adapter's parser, so a future change to an adapter's parsing can open a gap; the security review found a run of such forms, each now in `masks_every_drivers_literal_form`.
- Not run live: opening a saved `source` in the TUI.
