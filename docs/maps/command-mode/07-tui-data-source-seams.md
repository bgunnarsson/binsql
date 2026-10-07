---
title: Can the CLI manage data sources through the core as the TUI does, or is that logic locked in the app?
kind: research
mode: afk
status: resolved
blocked_by: []
claimed_by:
---

## Question

The CLI looks up and stores data sources the same way the TUI does. Before
designing its commands, establish where that logic lives. Trace everything
the TUI does to list, add, edit, rename, move between the user and project
files, remove, and set the default for a data source. For each step, say
whether it runs through `binsql-core` (`Workspace`, `Config`, `secrets`), and
so is callable from the CLI as it stands, or is done in the app layer
(`crates/binsql/src/app`, `ui`), and so would have to move into the core
first. That includes the `⌃N` form's rules: which file a new data source goes
to, keychain storage by default, rename moving the keychain entry, and
validation of names and drivers.

Also list what an agent would need that the TUI never does: listing data
sources with their scope and masked DSN, showing one, testing a connection
without running SQL.

## Context

- `crates/binsql-core/src/workspace.rs`, `config.rs`, `secrets/keychain.rs`,
  `secrets/mod.rs`.
- The `⌃N` / edit / disconnect handling under `crates/binsql/src/app` and
  `crates/binsql/src/ui`.
- `crates/binsql/src/cli/mod.rs`, `connect`, which already resolves names
  through `Workspace`.
- README: "Use", "Project data sources", "The keychain".

## Answer

The core already holds the parts that touch storage (Workspace scope, set, remove and move between files, keychain set/rename/delete, masking, resolution), but the save and delete sequences that tie them together and the form's validation are in the app. Move those into `binsql-core` before the CLI writes, and add the default setter and connection test, which nothing has today.

### Where each step runs

Paths `workspace.rs`, `config.rs` and `secrets/…` are under
`crates/binsql-core/src/`; `app/…` is under `crates/binsql/src/`.

| Step | Where it runs | Callable from the CLI today? |
| --- | --- | --- |
| List, qualified, merged across user and project | `Config::iter` (`config.rs:186`), `Config::listing` (:212) on the merged view, `Workspace::scope_of` (`workspace.rs:155`) | Yes |
| Resolve a bare name | `Config::resolve` (`config.rs:283`), exact first, then a unique leaf (`leaf_matches`, :293). CLI `connect` uses it (`cli/mod.rs:192`) | Yes |
| Which file a new source goes to | `Workspace::default_scope` (`workspace.rs:164`): back where it came from; a new one goes to the project when a `.binsql.json` is in play, else the user file. The form only reads it (`app/overlay.rs:308`, :328) | Yes |
| Write a source to a file; move it between files | `Workspace::set` (`workspace.rs:177`): writes the chosen layer, removes any copy in the other one, user file owner-only, project file shared | Yes |
| Remove | `Workspace::remove` (`workspace.rs:206`): from whichever file holds it | Yes |
| Keychain primitives | `secrets/keychain.rs`: `set` (:70), `get` (:59), `delete` (:79), `rename` (:87), `reference` (:55), `account` (:42), `is_reference` (:36) | Yes |
| Masked DSN | `binsql_core::config::mask_dsn` (`config.rs:355`), public in a public module, not re-exported at the crate root | Yes |
| Form validation: non-empty name, no `/` in name or folder, non-empty DSN, a driver inferred or chosen, refusing to switch a `keychain://` source off the keychain without a new DSN | `ConnectForm::build` (`app/overlay.rs:470`–528) | **No**, app only |
| Keychain by default for a new source | `ConnectForm::new` sets `keychain = true` (`app/overlay.rs:308`); `build` turns the DSN into a `keychain://<id>` reference plus a secret to store | **No**, app only |
| Save sequence: store the secret, carry it across on rename, write the config, remove the old name on rename | `App::save_data_source` (`app/mod.rs:991`–1037): `keychain::set` (:1002), `keychain::rename` (:1007), `Workspace::set` (:1014), `Workspace::remove(from)` (:1019) | **No**: the order matters (secret before config; old name removed without deleting its secret), and it lives in the app |
| Delete sequence: read the keychain account, remove from config, then delete the secret only once the config is written | `App::remove_data_source` (`app/mod.rs:1039`–1073), plus `schema_cache.forget_source` (:1068) | **No**, app only |
| Set the default | Nowhere. `Config.default` (`config.rs:101`) is read by `startup_sources` (:325) and merged by `Workspace` (`workspace.rs:132`), but no setter exists in core or app; it is only edited by hand | **No**, does not exist |
| Test a connection without SQL | Nowhere. `Session::open` (`session.rs:30`) resolves secrets and connects; nothing named ping or test exists | Opening a session is the nearest thing |

### What must move or be added before the CLI can write

1. **Validation** from `ConnectForm::build` into a core function the form and
   the CLI both call, so `source add` refuses the same names ⌃N does. Ticket
   18 adds the reserved-name check in `Workspace::set`; validation of name,
   folder and driver belongs beside it.
2. **The save sequence** (`App::save_data_source` minus tree, session and
   toast handling) into `Workspace`, for example a `save` that takes the id,
   the source, an optional secret, the previous id and the scope. The TUI
   then calls it and keeps its UI work.
3. **The delete sequence** (`App::remove_data_source` minus UI) likewise,
   keeping "config first, then the secret". Forgetting the schema cache is
   app state; the CLI has no schema cache to forget.
4. **A default setter** on `Workspace` that writes `default` into the file the
   source lives in. New behaviour, no TUI counterpart.
5. **A connection test**: opening a `Session` and dropping it is a test that
   runs no SQL. Some adapters run startup metadata queries on connect (ticket
   11 bounds those), so "no SQL" means no user SQL.

### What an agent needs that the TUI never does

- A list with each source's qualified name, scope (`user`/`project`), driver,
  read-only and open-on-start flags, whether it is the default, and a DSN
  that never shows a secret: `mask_dsn` for literals, and the reference
  itself for `keychain://` and Key Vault references, which hold no secret.
  Listing must not resolve secrets or connect.
- Showing one source the same way.
- Testing one: resolve its secret and connect, with no user SQL, reporting
  success or the failure.
- A DSN that reaches `add` without passing through argv (ticket 08 decides).

### Evidence

Read `workspace.rs:155`–222, `secrets/azure.rs:30`–50, `app/mod.rs:991`–1073
and `sql.rs:174`, and the scout's trace of `app/keys.rs`, `app/overlay.rs`,
`config.rs` and `secrets/keychain.rs` at the lines above. Nothing was run.
