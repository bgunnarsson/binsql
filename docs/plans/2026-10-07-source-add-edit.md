---
title: binsql source add and source edit save a data source from the command line, its secret in the keychain by default
date: 2026-10-07
status: in-progress
---

## Context

Ticket 33 of the command-mode map (`docs/maps/command-mode/33-source-add-and-source-edit.md`)
builds the `add` and `edit` rows of the contract in ticket 08's answer
(`docs/maps/command-mode/08-data-source-commands.md`, "## Answer"). `list`, `show`,
`default`, `test` and `clear-cache` exist in `crates/binsql/src/cli/source.rs`. Today a
data source can only be added with `⌃N` or edited with `e` in the TUI, or by hand in the
config; the README's Status section says v2's `binsql conn add` has no counterpart.

What the contract fixes:
- A connection string reaches `add`/`edit` through `--dsn-stdin` or `--dsn-env VAR`.
  `--dsn VALUE` is taken only for a reference or a sqlite path; any other literal is exit 2
  pointing at the other two. No DSN on `add` is exit 2. No prompt.
- Flags: `-d/--driver`, `--description`, `--readonly`/`--no-readonly`,
  `--open-on-start`/`--no-open-on-start`, `--scope user|project`, `--rename NEW` (edit),
  `--no-keychain`.
- Validation is `Draft::build` (`binsql-core/src/source.rs:50`); saving is
  `Workspace::save` (`workspace.rs:269`), which checks `check_new_id`, files the secret or
  renames it, then `set`s (moving between files) and drops the old name on a rename.
- A literal goes to the keychain by default; `--no-keychain` keeps it in the file, only in
  user scope.
- Scope left out is `Workspace::default_scope` (`workspace.rs:170`).
- `add` and `edit` print the saved row, as `show` does.

Decisions settled by the lead:
- The add/edit body takes `&impl SecretStore`; `run` passes `&Keychain`
  (`binsql_core::secrets::keychain::{Keychain, SecretStore}`, `keychain.rs:99-120`), as
  the TUI does (`app/mod.rs:994`). Unit tests in `cli/source.rs` use a recording store of
  their own; integration tests cover only paths that file nothing.
  Assumed, not asked: no env var or cfg redirects the binary's store — it would be a
  switch that writes secrets somewhere other than the credential store.
- Exit 2 (`usage`) for: a literal on `--dsn`; `add` over an existing name (pointing at
  `edit`); a reserved name (`check_new_id`'s message); `Draft::build`'s messages;
  `--no-keychain` with a literal in project scope; `edit` of an unknown name; no DSN on
  `add`; more than one of `--dsn`/`--dsn-stdin`/`--dsn-env`; a `--dsn-env` variable that is
  unset or empty. Exit 1 (`failed`) for a store or file write failure.
- Description is set on `saved.source` after `Draft::build` (which sets it empty,
  `source.rs:99`); `edit` keeps the stored one when `--description` is not given.
- The saved row prints through `row`/`table`.

Decided here:
- Assumed, not asked: a sqlite DSN (driver from `--driver`, else `Backend::infer`, else the
  stored driver on edit, is `Backend::Sqlite`) never goes to the keychain and may go to
  either file. `Draft.keychain` is false for it, and `--no-keychain` is accepted and
  changes nothing. — It is a file path and holds no secret, which is why the contract lets
  it on `--dsn`; filing a path in the credential store would only make it unreadable in
  the config.
- Assumed, not asked: a `keyvault://`/`azkv://`/vault-URL reference
  (`Reference::is_reference`, `secrets/reference.rs:25`) on `--dsn` is saved as written,
  keychain off, in either scope. — It holds no secret; `Draft::build` keeps any
  non-`keychain://` string as typed when the toggle is off (`source.rs:91`).
- Assumed, not asked: a `keychain://` string on `--dsn` is exit 2 for both `add` and
  `edit`: "a keychain:// reference names a secret binsql filed; pass the connection string
  with --dsn-stdin or --dsn-env". — `Draft::build` re-keys any keychain reference to
  `keychain://<id>` (`source.rs:88`) and `save` renames the store entry only from
  `previous` (`workspace.rs:285-289`), so on `add` the config would name an entry nothing
  filed, and on `edit` pointing at another source's entry would do the same. An edit that
  keeps its own reference does it by passing no DSN.
- Assumed, not asked: `edit` with no DSN builds the `Draft` from the stored DSN. A stored
  `keychain://` reference gives `keychain: true` (unless `--no-keychain`, which
  `Draft::build` refuses with its "To move this out of the …" message, exit 2), so a
  rename re-keys it and `save` carries the secret across. A stored literal or Key Vault
  reference gives `keychain: false`, so `edit` never silently files an existing literal.
  Moving a stored non-sqlite literal to project scope is exit 2 under the same rule as a
  new one. The driver is `--driver`, else the stored `backend` (not re-inferred). —
  Mirrors what the TUI form does with an untouched field.
- Assumed, not asked: `edit --rename NEW --scope S` combine in one `save`: `set` writes
  the new id into S and `save` then removes the old id from whichever file held it
  (`workspace.rs:293-299`). `--rename` onto a name that already exists (other than the
  source itself) is exit 2 "NEW already exists", since `save` would overwrite it.
- Assumed, not asked: `--scope project` with no `.binsql.json` in play is exit 2, checked
  before `save`. — `Workspace::set` refuses it (`workspace.rs:199-203`), but only after
  `save` has filed the secret, which would leave an orphan entry.
- Assumed, not asked: `check_new_id` is called before `save` and mapped to exit 2 with its
  message, so every refusal is usage and every error out of `save` is exit 1.
- Assumed, not asked: the positional NAME (and `--rename`'s value) splits at the first `/`
  into folder and name for the `Draft`; a second `/` gets `Draft::build`'s separator
  message. `add` checks the exact qualified id with `workspace.get`; `edit` finds the name
  with `find` (as `--conn` and `show` do).
- Assumed, not asked: giving both of a pair (`--readonly --no-readonly`) is exit 2.
  `add` defaults both switches to false, as the form does; `edit` keeps the stored values.
- Assumed, not asked: `--dsn-stdin` reads all of stdin through the existing `read_stdin`
  (`cli/mod.rs:376`) and trims trailing `\r`/`\n`; an empty result reaches
  `Draft::build`'s "needs a connection string".
- Assumed, not asked: an edit that files a new literal while renaming leaves the old
  name's keychain entry behind, and `--no-keychain` with a new literal over a keychain
  source leaves its entry too. — That is `save`'s behaviour for the TUI today
  (`workspace.rs:294-298`); changing it is outside this ticket.

## Relevant lore

- Notes (`notes.md`, 2026-10-07 13:32): integration tests must never reach `ratatui::init`.
  `binsql source add`/`edit` always has an op after `source`, so it stays in command mode.
- Notes (`notes.md`, 2026-10-07 13:58): no connect happens here, so the closed-port rule
  does not apply.

## Acceptance criteria

- `binsql source add NAME --dsn-stdin` / `--dsn-env VAR` saves a literal as
  `keychain://NAME`, files it through the store, and prints the row (dsn shown as the
  reference). `--no-keychain` in user scope keeps the literal in the user file and prints
  it masked.
- `binsql source add NAME --dsn ./x.db` and `--dsn keyvault://v/s` save as written, file
  nothing, and print the row; `--dsn postgres://u:p@h/db` is exit 2 naming
  `--dsn-stdin` and `--dsn-env`, and nothing is written.
- `add` over an existing id, a reserved top-level name, no DSN, two DSN flags, an unset or
  empty `--dsn-env` variable, `--no-keychain` with a non-sqlite literal in project scope,
  `--scope project` with no project file, and every `Draft::build` refusal are exit 2 with
  the exact message, and leave both files and the store unchanged.
- `binsql source edit NAME` with only some flags changes just those; the other fields,
  the scope and the DSN stay. `--scope` moves the source to the other file and out of the
  first. `--rename` moves it to the new id, carries a keychain entry (`rename OLD NEW` on
  the store), and refuses an existing target. An unknown NAME is exit 2.
- The add/edit-only flags on another op are exit 2; `--scope` is accepted on `default`,
  `add` and `edit`; `--rename` only on `edit`.
- HELP's command list and SOURCE section, and the README's Data sources section, describe
  `add` and `edit`; the README Status bullet no longer says `conn add` has no counterpart.
- `cargo test -p binsql --test command_mode`, `cargo test -p binsql --bins source` and
  `cargo clippy --workspace --all-targets` pass.

## Tasks

- [x] **CLI: `source add` and `source edit`, with their tests.** In `cli/source.rs`:
  - Extend `VALUES` with `dsn`, `dsn-env`, `driver`, `d`, `description`, `rename`, and
    `SWITCHES` with `dsn-stdin`, `readonly`, `no-readonly`, `open-on-start`,
    `no-open-on-start`, `no-keychain`. Widen the `--scope` guard to `default`, `add`,
    `edit`; refuse the new flags on other ops ("--X applies only to source add and source
    edit"), `--rename` outside `edit`. Update the "source needs a command" list.
  - In `run`, read the flags into a plain `Change` struct (DSN text already read from
    stdin/env, or `None`; driver via `Backend::parse` mapped to `usage`, as
    `cli/mod.rs:223-229`; description; `Option<bool>` for each pair; scope; rename;
    no_keychain). Reading stdin/env and the one-of-three check live here, not in the body.
  - `fn add(workspace: &mut Workspace, name: &str, change: &Change, store: &impl SecretStore) -> Result<String>`
    and `fn edit(...)` with the same shape, returning the saved id: build the `Draft`
    (decisions above), map its `Err` to `usage`, set `saved.source.description`, run the
    pre-checks (existing id / rename target, literal-on-`--dsn`, keychain:// on `--dsn`,
    project literal, project scope without a file, `check_new_id`) and then
    `workspace.save(saved, store)` mapped to `failed`. The literal-on-`--dsn` check needs
    to know the DSN came from `--dsn`, so `Change` records the source of the DSN.
  - The match arms `("add", [name])`/`("edit", [name])` load, call the body with
    `&Keychain`, and return `vec![row(&workspace, &id, source)]` into the existing print
    path; `("add" | "edit", _)` is "source add takes one name" (exit 2).
  - Unit tests in a new `#[cfg(test)] mod tests` (filter `source`): a `Recorder` like
    `workspace.rs:631-678` (records `set A S` / `rename A B`, optional failing), a
    `Workspace::load_at` over a temp dir with a user file and a project file. Cases:
    a stdin/env literal files `set NAME <literal>` and the config holds
    `keychain://NAME`; `--no-keychain` in user scope files nothing; `edit --rename` of a
    keychain source records `rename OLD NEW` and drops the old id; `edit` with a new
    literal re-files it; a failing store is exit 1 and leaves the config unwritten;
    every refusal records no store call.
  - Integration tests in `crates/binsql/tests/command_mode.rs` (nothing filed): reuse
    `Fixture`, `binsql_with_project` and `refused()`/`succeeds()`; read files back with
    `std::fs::read_to_string` as at `command_mode.rs:1009`. Add a `--dsn-env` case run
    with an extra `.env(...)` (add a small `binsql_with_env` helper beside
    `binsql_with_project`). Cases: a literal on `--dsn` is exit 2 and writes nothing;
    `add` over an existing name is exit 2 naming `source edit`; `add source` and
    `add list` at top level are exit 2 with `check_new_id`'s message, `add team/source`
    succeeds; `--dsn-env` literal with `--no-keychain --scope project` is exit 2;
    `--scope project` without a project file is exit 2; sqlite `add` prints the row in
    `-o json`; `add` with a keyvault:// reference saves it verbatim in project scope;
    `--dsn-env` with `--no-keychain` in user scope stores the literal and prints it
    masked; unset and empty `--dsn-env` are exit 2; `edit --description x` keeps driver,
    dsn, readonly, open_on_start and scope; `edit --scope project` moves a sqlite source
    out of the user file into `.binsql.json`; `edit --rename` of a sqlite source moves it;
    `edit` of an unknown name is exit 2; `Draft::build` messages (`"Could not tell the
    driver from that connection string — pick one"`, the `/` separator message) come
    through verbatim; `keychain://x` on `--dsn` is exit 2; `--readonly --no-readonly` is
    exit 2; `--rename` on `add` is exit 2.
  - Verify: `cargo test -p binsql --bins source`, `cargo test -p binsql --test command_mode`,
    `cargo clippy --workspace --all-targets`.

- [x] **Docs: HELP and README.** In `cli/mod.rs` HELP: add `binsql source add NAME` and
  `binsql source edit NAME` to the command list (`mod.rs:97-103`), and a paragraph in the
  SOURCE section (`mod.rs:143-162`) listing the DSN flags and their rule, the other flags,
  the keychain default, `--no-keychain` (user scope only), the default scope, `--rename`,
  and that both print the saved row. In `README.md`: the same in "### Data sources"
  (`README.md:367-400`), and rewrite the Status bullet at `README.md:619-624` to say
  `source add` and `source edit` cover v2's `conn add` (and that `source remove` is still
  to come, ticket 34). Verify: `cargo test -p binsql --test command_mode` (any HELP
  assertions) and read the rendered `binsql --help`.

## Files

- `crates/binsql/src/cli/source.rs`: flag lists, op dispatch, `Change`, `add`/`edit`
  bodies, unit tests. Reuses `load`, `find`, `row`, `table`, `usage`, `failed`, `note`,
  `print`, `read_stdin` (parent module), `Backend::parse`, `Draft`/`Saved`
  (`binsql_core::source`), `Workspace::{get, check_new_id, default_scope, has_project,
  scope_of, save}`, `keychain::is_reference`, `Reference::is_reference`.
- `crates/binsql/src/cli/mod.rs`: HELP command list and SOURCE section; `read_stdin`
  stays as is (reachable from the child module).
- `crates/binsql/tests/command_mode.rs`: an env-setting run helper and the integration
  tests above.
- `README.md`: Data sources section and the Status bullet.

## Verification

- `cargo test -p binsql --bins source`
- `cargo test -p binsql --test command_mode`
- `cargo clippy --workspace --all-targets`
- By hand, with `BINSQL_CONFIG` pointing at a scratch file:
  `binsql source add scratch --dsn ./scratch.db -o json` prints the row;
  `binsql source edit scratch --description demo` keeps the rest;
  `binsql source add bad --dsn postgres://u:p@h/db` exits 2 and the file is unchanged.
  Do not run a keychain-filing add by hand against the real store except deliberately.
