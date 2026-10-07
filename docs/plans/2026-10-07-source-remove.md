---
title: binsql source remove deletes a saved data source and then its keychain secret
date: 2026-10-07
status: in-progress
---

## Context

Ticket 34 (`docs/maps/command-mode/34-source-remove.md`) builds the step that
08 settled (`08-data-source-commands.md`, lines ~39, 55, 60, 99, 172-175):
`binsql source remove <name> --force` removes the source, then its keychain
secret once the config is written. Without `--force` it is exit 2. It prints
the removed row through the existing renderer. Ticket 20's
`Workspace::delete` (`crates/binsql-core/src/workspace.rs:324`) already
removes the config entry first and the secret second. Ticket 30's
`source add`/`edit` set the patterns this follows.

Assumed, not asked:

1. `Workspace::delete` is used as it is. A name defined in both the project
   and user files is removed from both, as ticket 20's delete does
   (`workspace.rs:304-320`). The printed row is the one `find` resolved.
2. `--force` is a new switch. Given to any other operation it is
   `--force applies only to source remove`, exit 2. Without `--force` the
   error is `source remove deletes NAME and its keychain secret; add --force
   to do it`, exit 2. This is checked before anything is loaded. An unknown
   name is exit 2 through `find`. Wrong arity is `source remove takes one
   name`.
3. If the secret delete fails after the config is written, the row is still
   printed. Then the command exits 1 with `failed("removed {id}, but its
   {STORE_NAME} entry is still there: {error}")`. The config change stands,
   and an agent has to see that the secret was left behind. The TUI handles
   the same case at `crates/binsql/src/app/mod.rs:1012-1034`.
4. The removal body is
   `fn remove(workspace, name, store: &impl SecretStore)`. It returns the row
   and the leftover-secret error, if there is one. A unit test can then drive
   it with the `Recorder` store.

## Relevant lore

- Integration tests must never reach `ratatui::init`, because it hangs on
  `/dev/tty`. `source remove` never reaches the TUI, so this does not apply.
- A connect failure must not point at a closed port. No test here connects,
  so this does not apply either.

## Acceptance criteria

- `binsql source remove NAME` without `--force` exits 2, and no config file changes.
- `binsql source remove NOPE --force` exits 2 with `no saved data source named NOPE`.
- `binsql source remove NAME --force` removes NAME from the file or files that held it. It prints the removed row, with its scope and default flag as they were before the removal, and exits 0.
- A keychain-reference source has its secret deleted only after the config is written. A plain connection string never touches the store.
- If the secret delete fails, the config is still changed and the row is printed. The command then exits 1 and says the entry is still there.
- `--force` given to any other source operation exits 2.
- HELP and README document `source remove`. README no longer says that it is missing.

## Tasks

- [x] `source remove` in `crates/binsql/src/cli/source.rs`, with unit and
  integration tests. Verify with `cargo test -p binsql --lib source` and
  `cargo test -p binsql --test command_mode source_`.
  - Add `"force"` to `SWITCHES`.
  - Add a guard beside the `--fresh` one (~98-101):
    `--force applies only to source remove`.
  - Add `remove` to the `source needs a command: ...` list (~69).
  - Add the arm `("remove", [name])`:
    - If `--force` is not set, return the usage error first.
    - Otherwise call `load()?`, then `remove(&mut workspace, name, &Keychain)?`.
    - Push the row, and keep the leftover-secret message.
  - Add the arm `("remove", _)`: `source remove takes one name`.
  - In the common tail, after printing and the notes, return
    `Err(failed(message))` when the leftover-secret message is set.
  - Add `fn remove(workspace: &mut Workspace, name: &str, store: &impl SecretStore) -> Result<((Vec<Value>, Option<String>), Option<String>)>`:
    - `find` the id.
    - Build `row(...)` before deleting. Scope and default need the entry
      still in the workspace.
    - Call `workspace.delete(&id, store)` and map an error to `failed`.
    - Map `Removed { secret_error: Some(e) }` to the message in decision 3.
  - Add unit tests in `mod tests`, using `Recorder` and `workspace(test)`:
    - For a reference source, the secret is deleted after the config is
      written, and the entry is gone.
    - A plain string makes no store call.
    - A failing store still removes the entry and returns the message.
    - An unknown name gives a usage error.
  - Add integration tests in `crates/binsql/tests/command_mode.rs`, copying
    the `source_add`/`source_edit` tests' use of `binsql_with_project`,
    `files`, and `saved_row`:
    - Without `--force` it exits 2, and the files are byte-identical.
    - An unknown name exits 2.
    - It removes from the project file and prints the row.
    - It removes from the user file and prints the row.
    - `source list --force` exits 2.
- [ ] Docs.
  - In HELP (`crates/binsql/src/cli/mod.rs` ~104-180), add the
    `source remove NAME --force` line.
  - In the README Data sources section (~401-420), say what it removes,
    the `--force` rule and the secret ordering.
  - Drop the Status bullet (~640) that says there is no `source remove` yet.
  - Write ticket 34's Answer.
  - Verify with `cargo test -p binsql` (HELP tests, if any) and by reading
    `binsql --help`.

## Files

- `crates/binsql/src/cli/source.rs`:
  - The switch, the guard, the dispatch arms, `fn remove` and the unit tests.
  - Reuses `find`, `row`, `load`, `failed`/`usage`, `Workspace::delete`,
    `STORE_NAME`, and the `Recorder` and `workspace` test helpers.
- `crates/binsql/tests/command_mode.rs`: integration tests built on the
  existing helpers.
- `crates/binsql/src/cli/mod.rs`: the HELP line.
- `README.md`: the Data sources text, and the Status bullet removed.
- `docs/maps/command-mode/34-source-remove.md`: the Answer, and status
  resolved.

## Verification

1. Run `cargo test -p binsql --lib source`,
   `cargo test -p binsql --test command_mode source_`, then the full
   `cargo test --workspace` and `cargo clippy --workspace --all-targets`.
2. By hand, in a scratch project:
   - `binsql source add tmp --dsn sqlite::memory:`.
   - `binsql source remove tmp` exits 2.
   - `binsql source remove tmp --force` prints the row.
   - `binsql source list` no longer shows it.
