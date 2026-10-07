---
title: Verb names are reserved for new top-level data sources, and `binsql -- <name>` opens any saved name
date: 2026-10-07
status: in-progress
---

## Context

Ticket [18](../maps/command-mode/18-verb-reservation.md) builds decisions 3
and 4 of [05](../maps/command-mode/05-verb-namespace.md), ahead of the `source`
verb. Today `binsql query` always means command mode (`main.rs:40`), so a
data source saved as top-level `query` cannot be opened by name, and nothing
stops a new one being saved. Outcome: the verb list lives in `binsql-core`,
`Workspace::set` refuses a new top-level verb name, and `--` makes the next
argument a TUI target.

Settled: unattended loop; work stays on `main`, each task committed and pushed;
no AI mentions in commits. Additions only: no existing verb, flag, message or
exit code changes, and `is_verb` keeps its meaning for `query`, `exec`,
`inspect`. Adding `source` to the list is not this plan's job.

## Relevant lore

None found. (No `docs/solutions` in this repo.) Project note: command-mode
`--limit` is client-side; not touched here.

## Acceptance criteria

- `binsql_core::RESERVED_NAMES` is `["query", "exec", "inspect"]`, and
  `cli::is_verb` answers from it; `cli::VERBS` is gone or an alias of it.
- `Workspace::set("query", ..)` fails with a config error when no `query` is
  saved in the user or project file, and writes nothing.
- `Workspace::set("folder/query", ..)` succeeds.
- `Workspace::set("query", ..)` succeeds when `query` is already saved (either
  layer), including a move between layers.
- `⌃N` saving a new top-level `query` shows the refusal in the form, as other
  save errors are shown, and files no keychain secret under `query`.
- `parse_args(["--", "query"])` gives target `query`; `binsql -- query` reaches
  `parse_args` because `--` is not a verb.
- The four existing `main.rs` tests pass unchanged.
- `HELP` and `README.md` document `binsql -- <name>`.

## Tasks

- [x] **1. Reserved names in core, refused by `Workspace::set`.**
  In `crates/binsql-core/src/workspace.rs` add
  `pub const RESERVED_NAMES: [&str; 3] = ["query", "exec", "inspect"];` with a
  doc comment (the bare names `binsql <verb>` takes; a new top-level data
  source may not use one, a folder entry may). Add
  `pub fn check_new_id(&self, id: &str) -> Result<()>`: error when
  `!id.contains('/') && RESERVED_NAMES.contains(&id) && self.scope_of(id).is_none()`,
  via `Error::config(anyhow::anyhow!(...))` as at `:181`. Message:
  `"{id} is a binsql command; save it inside a folder, such as folder/{id}, or under another name"`.
  `scope_of` (`:155`) looks only at the saved layers, so an ephemeral entry
  never counts as saved. Call it as the first line of `set` (`:177`), before
  any layer is touched. Re-export: `pub use workspace::{RESERVED_NAMES, Scope, Workspace};`
  in `lib.rs:31`.
  Tests in `workspace.rs`'s `mod tests`, using the `workspace(test, user, project)` helper and `source(dsn)`:
  - `a_new_top_level_verb_name_is_refused` — `set("query", .., Scope::User)` is
    `Err`, and `workspace.get("query")` stays `None`.
  - `a_verb_name_inside_a_folder_is_saved` — `set("folder/query", ..)` is `Ok`
    and `get("folder/query")` is `Some`.
  - `a_saved_top_level_verb_name_stays_editable` — user JSON holding top-level
    `query`; `set("query", .., Scope::User)` is `Ok`; and with a project,
    `set("query", .., Scope::Project)` is `Ok` (a move between layers).
  Verify: `cargo test -p binsql-core`.

- [x] **2. `cli::is_verb` reads the core list.**
  `crates/binsql/src/cli/mod.rs:46`: delete `const VERBS`; `is_verb` returns
  `binsql_core::RESERVED_NAMES.contains(&name)`. Update the comment at `:65`
  to say "the reserved names in core" rather than "the two lists". Add to
  `cli/mod.rs` tests (create `#[cfg(test)] mod tests` if none):
  - `the_verbs_are_the_reserved_names` — `is_verb` is true for each of
    `query`, `exec`, `inspect`, false for `source`, `eimskip/prod`, `--`.
  Verify: `cargo test -p binsql`.

- [x] **3. `⌃N` checks before filing the secret.**
  `App::save_data_source` (`crates/binsql/src/app/mod.rs:991`) writes the
  keychain (`:1002`, `:1007`) before `config.set` (`:1014`), so a refused name
  would leave a secret filed under `query`. Add as its first statement after
  destructuring: `self.config.check_new_id(&id).map_err(|error| error.to_string())?;`.
  The error string reaches the form through the existing path
  (`keys.rs:392` `save_form`, rendered at `ui/overlays.rs:801`); no UI change.
  `set` keeps its own check, so `source add` later is covered too.
  No new test (it needs the keychain); covered by task 1's tests and a manual
  check in Verification.
  Verify: `cargo build -p binsql && cargo test -p binsql`.

- [ ] **4. `parse_args` accepts `--`.**
  `crates/binsql/src/main.rs:237`: add a `positional` bool. A `"--"` arm,
  placed before the `starts_with('-')` arm and guarded `if !positional`, sets
  `positional = true`. Guard the option arms (`-h`, `-V`, `--debug-keys`,
  `-d`, unknown option) with `!positional`, or equivalently match on
  `(positional, arg)`: after `--`, every argument is positional, so
  `binsql -- -weird-name` is a target. The target/unexpected arms are
  unchanged, so a second positional is still "Unexpected argument". `main.rs:40`
  needs no change: `--` is not a verb. Tests in `main.rs`'s `mod tests`:
  - `a_double_dash_makes_a_verb_a_target` — `["--", "query"]` gives target
    `Some("query")`.
  - `options_before_a_double_dash_still_apply` — `["-d", "sqlite", "--", "exec"]`
    gives backend `Some(Backend::Sqlite)`, target `Some("exec")`.
  - `a_double_dash_ends_options` — `["--", "--help"]` is `Some(..)` with target
    `Some("--help")` (does not print help).
  - `a_double_dash_alone_opens_the_list` — `["--"]` gives target `None`.
  - `a_double_dash_allows_one_target` — `["--", "a", "b"]` is `Err`.
  Existing `parses_a_bare_target`, `parses_an_explicit_driver`,
  `help_stops_before_starting`, `rejects_unknown_options` pass unchanged.
  Verify: `cargo test -p binsql`.

- [ ] **5. Document `--`.**
  `HELP` (`main.rs:15`): under USAGE add
  `binsql -- <connection>     open a saved data source named like a command`.
  `README.md:42-54`: add `binsql -- query                 # a saved data source named like a verb`
  to the block, and to the paragraph after it a sentence: `--` makes the next
  argument a data source even when it names a verb, and a new top-level data
  source cannot take a verb's name (inside a folder it can; one already saved
  stays editable).
  Verify: `cargo build --release && ./target/release/binsql --help` shows the line.

## Files

- `crates/binsql-core/src/workspace.rs`: `RESERVED_NAMES`, `check_new_id`, the
  check in `set`; reuses `scope_of` and `Error::config`; three tests.
- `crates/binsql-core/src/lib.rs`: re-export `RESERVED_NAMES`.
- `crates/binsql/src/cli/mod.rs`: `is_verb` reads `RESERVED_NAMES`; `VERBS`
  removed; one test.
- `crates/binsql/src/app/mod.rs`: early `check_new_id` in `save_data_source`.
- `crates/binsql/src/main.rs`: `--` in `parse_args`, `HELP` line, five tests.
- `README.md`: usage line and paragraph.

## Verification

- `cargo test -p binsql-core && cargo test -p binsql`, then `cargo build --release`.
- `./target/release/binsql --help` lists `binsql -- <connection>`.
- With a temp config (`BINSQL_CONFIG=<tmp>/c.json BINSQL_PROJECT=`) holding a
  top-level sqlite `query`: `binsql -- query` opens it in the TUI;
  `binsql query` still runs command mode, with the same message and exit code as before this change.
- In the TUI, `⌃N` with name `query` shows the refusal in the form and the
  form stays open; name `scratch/query` saves.
