---
title: Move data-source validation, save and delete from the app into the core
kind: task
mode: afk
status: open
blocked_by: [18]
claimed_by:
---

## Question

Build what 07 found missing before the CLI can write data sources, without
changing what `⌃N` or delete does in the TUI:

- A core function that validates a data source as `ConnectForm::build`
  (`crates/binsql/src/app/overlay.rs:470`–528) does: non-empty name, no `/`
  in name or folder, non-empty DSN, a driver inferred or chosen, and refusing
  to switch a `keychain://` source off the keychain without a new DSN. The
  form calls it and shows its errors as today.
- A `Workspace` save that takes the id, the source, an optional secret to
  store in the keychain, the previous id and the scope, and runs the sequence
  in `App::save_data_source` (`crates/binsql/src/app/mod.rs:991`–1037):
  secret first, carry it across on rename, write the config, remove the old
  name without deleting its secret. The app calls it and keeps its tree,
  session and toast work.
- A `Workspace` delete that runs `App::remove_data_source`
  (`app/mod.rs:1039`–1073) minus UI: read the keychain account, remove from
  config, then delete the secret once the config is written. The app keeps
  `schema_cache.forget_source`.
- Tests in the core for each sequence: rename keeps the secret, delete
  removes the secret only after the config, validation errors match the
  form's. Existing app tests still pass.

Keychain writes in tests must not touch the real store; follow what the
existing keychain tests do.

## Context

- 07's answer (`07-tui-data-source-seams.md`) has the table of where each
  step runs.
- 18 adds the reserved-name refusal in `Workspace::set`; this ticket builds
  on it, so it waits.
- The default setter and the connection test wait on 08's command shape.

## Answer
