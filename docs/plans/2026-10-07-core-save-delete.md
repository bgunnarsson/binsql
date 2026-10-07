---
title: Data-source validation, save and delete run in binsql-core, and the TUI calls them
date: 2026-10-07
status: in-progress
---

## Context

Ticket 20 of the command-mode map (`docs/maps/command-mode/20-core-save-delete.md`).
Map decision 07 found that the core holds every storage step but that
validation and the save and delete sequences live in the app
(`ConnectForm::build`, `crates/binsql/src/app/overlay.rs:470`–528;
`App::save_data_source`, `crates/binsql/src/app/mod.rs:991`–1041;
`App::remove_data_source`, `app/mod.rs:1043`–1077). The `binsql source` verb
(tickets 30–34) needs them in `binsql-core` before the CLI can write a data
source. Ticket 18 is built: `Workspace::check_new_id` refuses a new top-level
source named like a verb (`crates/binsql-core/src/workspace.rs:179`), and
`Workspace::set` calls it first (:194).

Decisions taken for this plan (the user is not available; the option that
changes least was taken each time):

- **Validation moves to core as a plain-data input and a build function.** A
  new module `binsql_core::source` holds `Saved` (moved verbatim from
  `overlay.rs:270`) and `Draft`, a struct of what the form holds (folder, name,
  dsn, backend option, keychain flag, read-only, open-on-start, previous,
  scope) with `Draft::build(&self) -> Result<Saved, String>`. The body is
  `ConnectForm::build`'s, moved unchanged, with `effective_backend` inlined as
  `self.backend.or_else(|| Backend::infer(self.dsn))` on the untrimmed DSN, as
  today (`overlay.rs:389`). The error type stays `String` so the five messages
  are byte-for-byte what the form shows. `ConnectForm::build` becomes a call to
  it. No new CLI surface.
- **The keychain seam is a small trait, not "test only the non-keychain
  paths".** The ticket's required tests (rename keeps the secret, delete
  removes the secret only after the config) are the keychain paths; without a
  seam they could not be tested at all, since the only keychain round trip
  (`crates/binsql-core/tests/keychain_roundtrip.rs`) is `#[ignore]` and the
  in-crate keychain tests touch only pure functions
  (`secrets/keychain.rs:110`). The trait is `SecretStore` with the three
  writes the sequences use (`set`, `rename`, `delete`), and a unit struct
  `Keychain` implementing it by calling the existing free functions
  (`keychain.rs:70`, :87, :79), which stay as they are. `Workspace::save` and
  `Workspace::delete` take `&impl SecretStore`; the app passes `&Keychain`.
  Cost: one parameter on two methods and one trait; nothing else changes.
- **Workspace runs the sequences without UI.**
  - `Workspace::save(&mut self, saved: Saved, store: &impl SecretStore) -> Result<()>`:
    `check_new_id(id)`; then the secret (set it; or, with no secret, a
    `keychain://` DSN and a rename, `store.rename(from, id)`); then
    `self.set(id, source, scope)`; then, on a rename, `self.remove(from)` and
    never `store.delete(from)`. The comments at `app/mod.rs:1000`, :1007 and
    :1022 move with the code.
  - `Workspace::delete(&mut self, id: &str, store: &impl SecretStore) -> Result<Option<Removed>>`:
    read `keychain::account(&source.dsn)` before removal; `self.remove(id)?`;
    `Ok(None)` when nothing was there; otherwise `store.delete(account)` only
    now, and `Ok(Some(Removed { secret_error }))` with
    `secret_error: Option<Error>` holding a failed secret delete, so the app
    can warn `Removed {name}, but {error}` as today (`app/mod.rs:1069`).
    `Removed` lives in `workspace.rs`.
- **The app keeps only UI work.** `save_data_source` computes `is_new` and
  `renamed_from` before calling `self.config.save(saved, &Keychain)`, maps the
  error with `to_string()` (the same text as today), then on a rename removes
  the session and the tree node, adds the tree node when new, toasts and
  connects. `remove_data_source` removes the session and reads `source_id`
  first, as today, calls `self.config.delete(name, &Keychain)`, and reports:
  `Err` → `Saving connections: {error}`; `Ok(None)` → nothing;
  `Ok(Some(removed))` → the secret warning if any, `schema_cache.forget_source`,
  `tree.remove_source`, `Removed {name}`.

One ordering difference, accepted: on a rename, `sessions.remove(from)` now
runs after the old config entry is removed rather than just before it
(`app/mod.rs:1025`). It only differs when that removal fails, where the old
session now stays open beside the error. Nothing user-visible otherwise.

## Relevant lore

None found.

## Acceptance criteria

- `binsql_core::source::Draft::build` returns, for the same inputs, exactly
  what `ConnectForm::build` returned, including the five error messages
  verbatim; `ConnectForm::build` calls it and the existing overlay tests
  (`overlay.rs:569`–652) pass unchanged.
- `Workspace::save` files a new secret before writing the config, writes
  nothing when the secret cannot be filed, refuses a reserved new id before
  touching the store, carries a stored secret across a rename with
  `rename` and never deletes the old name's secret.
- `Workspace::delete` deletes a `keychain://` source's secret only after the
  config file no longer lists the source, leaves the store alone for a
  non-reference DSN or a missing id, and reports a failed secret delete in
  `Removed` with the config entry already gone.
- No test writes to the real credential store; all new core tests use a
  recording `SecretStore`.
- `App::save_data_source` and `App::remove_data_source` call the core methods
  and keep their toasts, tree, session and schema-cache work with today's
  messages. `cargo test --workspace` and `cargo build --release` pass.

## Tasks

- [x] **Validation in core.** Add `crates/binsql-core/src/source.rs` with
  `Saved` (moved from `overlay.rs:270`, docs kept) and `Draft<'a>` (borrowed
  `&str` fields for folder, name, dsn; `backend: Option<Backend>`,
  `keychain`, `read_only`, `open_on_start: bool`; `previous: Option<String>`;
  `scope: Scope`) with `build` holding `overlay.rs:471`–527's body. Register
  `pub mod source;` in `lib.rs`. Tests: each of the five errors by exact
  string (empty name, separator in name, separator in folder, empty DSN,
  undetectable driver, reference with the keychain off), plus the three
  storage cases (typed + keychain on → reference and secret; typed + off →
  string, no secret; untouched reference → re-keyed reference, no secret) and
  folder qualification.
  Verify: `cargo test -p binsql-core source::`
- [ ] **The form delegates.** `ConnectForm::build` builds a `Draft` from its
  fields (`previous: self.editing.clone()`) and returns `draft.build()`;
  remove `Saved` from `overlay.rs` and import it from `binsql_core::source`
  in `overlay.rs` and `app/mod.rs:25`. `effective_backend` stays for the
  display code.
  Verify: `cargo test -p binsql overlay`
- [ ] **The secret-store seam.** In `secrets/keychain.rs`, add
  `pub trait SecretStore { fn set(&self, account: &str, secret: &str) -> Result<()>; fn rename(&self, from: &str, to: &str) -> Result<()>; fn delete(&self, account: &str) -> Result<()>; }`
  and `pub struct Keychain;` implementing it via the free functions.
  Verify: `cargo test -p binsql-core keychain`
- [ ] **`Workspace::save`.** Add it after `set` in `workspace.rs`. In the test
  module add a `Recorder` store: a `RefCell<Vec<String>>` call log, an
  optional failure for `set`, and the user config path so `delete` can record
  whether the file still names the account when it is called. Tests, named
  `save_…`: a new keychain source logs `set` and the config then lists it;
  a failing `set` returns the error and the config file is unwritten; a
  reserved new id (`query`) errors with no store call; a rename of a
  `keychain://` source with no new secret logs `rename(old, new)`, lists the
  new id, drops the old one and logs no `delete`; a rename with a new secret
  logs only `set(new)`; a plain DSN with no secret makes no store call.
  Verify: `cargo test -p binsql-core workspace::tests::save_`
- [ ] **`Workspace::delete` and `Removed`.** Add both. Tests, named
  `delete_…`: a `keychain://` source logs `delete(account)` and the recorder
  saw the config file without the id at that moment; a plain DSN logs
  nothing; a missing id returns `Ok(None)` and logs nothing; a failing store
  `delete` returns `Some(Removed { secret_error: Some(_) })` and the config no
  longer lists the id.
  Verify: `cargo test -p binsql-core workspace::tests::delete_`
- [ ] **The app calls them.** Rewrite `App::save_data_source` and
  `App::remove_data_source` (`app/mod.rs:991`–1077) as described in Context,
  importing `keychain::Keychain`; keep the doc comment on save and the
  comment on reading `source_id` before removal.
  Verify: `cargo test -p binsql app`
- [ ] **Whole workspace.**
  Verify: `cargo test --workspace` and `cargo build --release`

## Files

- `crates/binsql-core/src/source.rs` (new): `Draft`, `Draft::build`, `Saved`;
  body moved from `ConnectForm::build`, using `config::{SEPARATOR, qualify}`,
  `Backend::infer`, `keychain::{is_reference, reference, STORE_NAME}`.
- `crates/binsql-core/src/lib.rs`: `pub mod source;`.
- `crates/binsql-core/src/secrets/keychain.rs`: `SecretStore` trait and
  `Keychain` over the existing `set`/`rename`/`delete`.
- `crates/binsql-core/src/workspace.rs`: `save`, `delete`, `Removed`; reuses
  `check_new_id`, `set`, `remove`; tests reuse the `source(dsn)` and
  `workspace(test, user, project)` helpers (:285, :297).
- `crates/binsql/src/app/overlay.rs`: `Saved` removed; `build` delegates.
- `crates/binsql/src/app/mod.rs`: import of `Saved` from core;
  `save_data_source` and `remove_data_source` keep only UI work.

## Verification

`cargo test --workspace` and `cargo build --release` pass, and no test touches
the real store: `grep -n "Keychain" crates/binsql-core/src/workspace.rs` shows it
only outside `mod tests`. By hand, in the TUI on a scratch config: `⌃N` a new
source with the keychain on (saves, connects, `Saved <id>`), rename it (still
connects, so the secret came across), switch a stored source's keychain off
without retyping (the same refusal as before), and delete it (`Removed <id>`,
and the `binsql` entry is gone from the credential store).
