---
title: binsql source default prints the default data source and sets it in the file that decides it
date: 2026-10-07
status: in-progress
---

## Context

Ticket 31 of the command-mode map (`docs/maps/command-mode/31-*.md`). The
contract is 08's answer: the `source default [<name>]` row of Ops, Rows, and
Setting the default. Map rules apply: stdout carries only `--format` output,
notes go to stderr, exit 0/1/2, the README and HELP document every flag.
Nothing sets a default today, the TUI included; `Config.default`
(`config.rs:98-104`, `Option<String>`, skipped when `None`) is only read.

Settled by the lead (the option that changes least; the user asked for
routine calls to be made without asking):

- `source default` with no name prints the merged default's row. With no
  default set it prints nothing on stdout, in any format, and exits 0.
- `source default NAME` resolves NAME through `find` (`cli/source.rs:63`), as
  `show` does: unknown or ambiguous is exit 2. It writes the qualified id and
  prints the source's row, whose `default` column is now `true`.
- `--scope user|project` picks the file; left out, the source's own file
  (`Workspace::scope_of`, `workspace.rs:161`).
- Writing the user file while the project file has a default that is not
  this source is exit 1, with a message to pass `--scope project`.
  `--scope project` with no project file is refused as `set` refuses it
  (`workspace.rs:199-203`), exit 1.
- A bad `--scope` value is exit 2; more than one name is exit 2.

Design decided here:

- **Core.** `Workspace::set_default(&mut self, id: &str, scope: Option<Scope>)
  -> Result<()>`, beside `set` (`workspace.rs:195`):
  1. `let own = self.scope_of(id)`. `None` (unknown, or an ephemeral source,
     which lives in neither file) is refused whatever the scope:
     `{id} is not saved in a file, so it cannot be the default`. A default
     naming a source that exists only for this run would dangle on the next
     one; refusing is less surprising than writing it. (`binsql source` never
     adds ephemerals, so the CLI never reaches this; the core test does.)
  2. `let scope = scope.or(own).expect(...)` — `own` is `Some` here.
  3. `Scope::User`: if the project's `default` is `Some(d)` and
     `self.merged.resolve(&d).as_deref() != Some(id)`, refuse:
     `this project's {PROJECT_FILE} sets the default to {d}, which wins over
     your config; pass --scope project to change it there`. A project default
     already naming this source is no conflict; the user file is written.
     Then `self.user.default = Some(id.into())`.
  4. `Scope::Project`: `self.project.as_mut()`, else the same error `set`
     gives (`there is no {PROJECT_FILE} here to save the default in`); set its
     `default`. The user file is left alone.
  5. `self.write(scope)?; self.rebuild();`
  Errors are `Error::config(anyhow!(...))`, as `set` and `check_new_id` do.
  The id stored is the qualified one, so a later folder entry with the same
  leaf cannot make it ambiguous.
- **CLI.** `VALUES` in `cli/source.rs` gains `"scope"`. In `run`, read it
  once: `None`, `"user"` → `Scope::User`, `"project"` → `Scope::Project`,
  anything else exit 2 `--scope takes user or project`. Given on `list` or
  `show` it is exit 2 `--scope applies only to source default`. Arms:
  - `("default", [])`: `workspace.default` put through `resolve`; `Some(id)`
    gives the one row; `None` with no default set gives no rows; a default
    that no longer resolves gives no rows and a stderr note
    `note: the default {name} is not a saved data source` (through `note`,
    silent under `-o none`).
  - `("default", [name])`: `find`, then `workspace.set_default(&id, scope)`
    mapped to `failed(error.to_string())` (exit 1), then the row.
  - `("default", _)`: exit 2 `source default takes at most one name`.
  `print` is skipped when there are no rows, so "nothing" is literally no
  stdout, not an empty table or `[]`. The usage message for no op becomes
  `source needs a command: list, show or default`.
- No new crate exports: `Scope` and `Workspace` are already public.

## Relevant lore

None found. Project note applies to the CLI tests: integration tests must
never reach `ratatui::init`; every `source default` form ends before it.

## Acceptance criteria

- `binsql source default` prints the default's row (a bare-leaf default such
  as `inspect` shows `team/inspect`) in every `-o` format; with no default set
  it prints nothing on stdout and exits 0.
- `binsql source default NAME` writes the qualified id into the source's own
  file, or the `--scope` file, and prints its row with `default: true`; a
  following `source default` prints the same row.
- With a project default naming another source, writing the user file is
  exit 1 and the stderr names `--scope project`; neither file changes.
  `--scope project` then succeeds and only the project file changes.
- `--scope project` with no project file is exit 1 and writes nothing.
- An unknown or ambiguous NAME, two names, and `--scope other` are exit 2;
  `source list --scope user` is exit 2.
- HELP's SOURCE section and the README's `### Data sources` cover the op and
  `--scope`.

## Tasks

- [x] **Core.** `Workspace::set_default` as above, with tests in
  `workspace.rs`'s module using `workspace()` (`:365`) and reading the files
  back with `Config::load_from`:
  - user scope: `set_default("scratch", None)` writes `default: "scratch"`
    to the user file, the merged `default` is `scratch`, and no project file
    is written when there is none;
  - project scope: `set_default("eimskip/local", None)` writes the project
    file and leaves the user file's `default` as it was; `--scope`-style
    `Some(Scope::Project)` for a user source writes the project file too;
  - the shadowing refusal: a project with `"default": "eimskip/local"`,
    `set_default("scratch", None)` errs mentioning `--scope project`, and
    the user file on disk is unchanged; `Some(Scope::Project)` then succeeds;
    a project default already naming the source does not refuse;
  - `Some(Scope::Project)` with no project errs naming `PROJECT_FILE`
    (as `:498` does for `set`);
  - an ephemeral source and an unknown id are refused.
  Verify: `cargo test -p binsql-core workspace`, clippy.
- [x] **CLI.** The `default` arms, `--scope` parsing and the empty-print
  rule in `cli/source.rs`. Integration tests in `tests/command_mode.rs`,
  through `write_sources` (`:865`, user default `inspect`, project without a
  default) and `binsql_with_project` (`:65`):
  - `source default -o json` gives one row, `team/inspect`, `default: true`;
  - `source default pg` succeeds with `pg`'s row (password still masked);
    the user config on disk now has `"default": "pg"`; `source default -o
    json` gives `pg`;
  - a project file rewritten with `"default": "here"`: `source default pg`
    is `failed()` with stderr containing `--scope project`, and the user file
    still says `inspect`; `source default pg --scope project` succeeds and
    the project file says `pg`;
  - a config with no `default`: `source default` succeeds with empty stdout;
  - `source default nope`, `source default pg kc`, `source default pg
    --scope other` and `source list --scope user` are `refused()`.
  Verify: `cargo test -p binsql --test command_mode source`, clippy.
- [ ] **Docs.** HELP's SOURCE section (`cli/mod.rs`): `source default
  [NAME] [--scope user|project]`, that no name prints the current default's
  row (nothing when none is set), that NAME writes the source's own file
  unless `--scope` names one, and that the project file's default wins so a
  user write under a differing project default is refused. The same in the
  README's `### Data sources`; adjust the `## Status` bullet so setting the
  default is listed as working.
  Verify: `cargo test -p binsql`, clippy, and read both sections.

## Files

- `crates/binsql-core/src/workspace.rs`: `set_default`, reusing `scope_of`,
  `write`, `rebuild` and the merged `resolve`; tests beside
  `saving_to_a_project_that_does_not_exist_is_refused`.
- `crates/binsql/src/cli/source.rs`: `"scope"` in `VALUES`, the scope
  parser, the three `default` arms, skipping `print` with no rows; reuses
  `find`, `row`, `table`, `note`, `failed`, `usage`.
- `crates/binsql/tests/command_mode.rs`: the integration tests above.
- `crates/binsql/src/cli/mod.rs`: HELP SOURCE section.
- `README.md`: `### Data sources` and the Status bullet.

## Verification

- `cargo test -p binsql-core workspace`, `cargo test -p binsql --test
  command_mode source`, and `cargo clippy --workspace --all-targets` with no
  warnings.
- By hand, with a scratch `BINSQL_CONFIG` and `BINSQL_PROJECT`: `binsql
  source default`, `binsql source default <name>`, `binsql source default -o
  json --pretty`; add a `default` to the project file and check the user
  write is refused with the `--scope project` hint and that `--scope project`
  changes only the project file; `binsql source list` shows `default: true`
  on the new one.
