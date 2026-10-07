---
title: binsql source test reports the stage a connection fails at, --fresh bypasses the secret cache, and source clear-cache empties it
date: 2026-10-07
status: in-progress
---

## Context

Ticket 32 of the command-mode map
(`docs/maps/command-mode/32-source-test-fresh-and-clear-cache.md`). The
contract is 08's answer: the `source test` and `source clear-cache` rows of
Ops, "Connection test", and "Bypassing and clearing the cache". Map rules
apply: stdout carries only `--format` output, notes and errors go to stderr,
exit 0/1/2, and HELP and the README document every flag.

Settled by the lead (the option that changes least; the user asked for
routine calls to be made without asking):

- `source test [NAME] [--fresh]`. NAME resolves through `find`
  (`cli/source.rs:104`), as `show` does: unknown or ambiguous is exit 2. With
  no name it tests the default, exactly as `connect` picks it
  (`cli/mod.rs:246-261`), with the same messages and exits: no default is
  exit 2, and a default that no longer resolves is exit 1.
- It prints one row, `name, ok, stage, elapsed_ms, error`, through the
  renderer. Exit 0 when it is ok. On failure it exits 1: the row still goes to
  stdout and an `error:` line goes to stderr (through `failed`, which `main`
  prints as `error: …`, `cli/mod.rs:80`).
- The probe lives in the core. The CLI only times it and formats the result.
- `--fresh` uses `Resolver::resolve_fresh`, which skips the cache read and
  writes the cache on success. Given to any op but `test`, it is exit 2.
- `source clear-cache` calls `Cache::clear` on the cache `Resolver::from_env`
  builds. It prints nothing on stdout, exits 0, and takes no name.

Design decided here:

- **The secret never reaches the error text.** Every adapter builds its
  connect error as `Error::connect(dsn, e)`, and that displays as
  `connecting to {dsn}: …` (`error.rs:11-16`; `sqlite.rs:39,53`,
  `postgres.rs:48,122`, `mysql.rs:38,102`, `mssql.rs:286-292,705`).
  `Error::UnknownBackend(dsn)` at `mysql.rs:104` and `mssql.rs:737` carries
  the whole DSN as well, so it gets the same treatment. Here the `dsn` is the
  *resolved* string, which is the secret. The probe takes the error's
  `Display` and replaces every occurrence of the resolved DSN (and of its
  trimmed form, if that differs) with `mask_dsn(source.backend, &dsn)`
  (`config.rs:355`, tested for each driver in ticket 30). One rule covers
  every variant and keeps the familiar `connecting to …: <driver error>`
  shape. A stored literal is masked the same way, because it is the same
  string. `secret`-stage text holds only a reference or a vault URL, so it
  goes out as is. The `token` stage's name is `"azure ad"` (`mssql.rs:830-855`),
  so it holds no DSN.
- **Stages.** `secret` is the `resolve`/`resolve_fresh` error. `token` is an
  adapter error matching `Error::Connect { name, .. }` with
  `name == "azure ad"`: `azure_cli_token` is the only code that uses that
  name, and it runs inside `adapter::connect` through `build_config`
  (`mssql.rs:697-710`). Any other adapter error is `connect`. That closes 08's
  "unchecked" point: the token error can be told apart by name.
- **`stage` on success** is `null`, and so is `error`. A null holds up in
  every format, and an agent reads `ok` first.
- **`elapsed_ms`** covers the whole probe (resolve and connect). The CLI
  measures it with `Instant`, so the core result carries no timing.
- **The keychain case of the secret-stage test** would read the real OS
  keychain (`secrets/mod.rs:65-67`, no seam), so it is left out. The
  bare-vault-URL case (`secrets/mod.rs:73`) covers the secret stage. A
  missing keychain entry fails in the same `?`, so it adds no coverage of
  stage mapping.

### Core

`crates/binsql-core/src/secrets/mod.rs`:

- Move the body of `resolve` into a private
  `async fn resolve_with(&self, dsn: &str, read_cache: bool) -> Result<String>`.
  The only change is that it guards `self.cache.get` (`:84`) with `read_cache`.
  The keychain branch, the literal and vault-URL handling, and the cache
  `put` on success all stay as they are.
- `pub async fn resolve(&self, dsn)` calls `resolve_with(dsn, true)`.
  `pub async fn resolve_fresh(&self, dsn)` calls `resolve_with(dsn, false)`,
  with a doc comment: it skips the cached copy and refreshes it on success,
  for when a secret was rotated before the TTL ran out.

`crates/binsql-core/src/session.rs`, beside `open_with`:

```rust
/// Where a connection test stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage { Secret, Token, Connect }

impl Stage {
    pub fn as_str(self) -> &'static str { /* "secret" | "token" | "connect" */ }
}

/// A failed connection test: the stage, and an error text holding no secret.
#[derive(Debug)]
pub struct ProbeFailure { pub stage: Stage, pub message: String }

impl Session {
    /// Resolves the source's secret and opens one connection, then drops it.
    /// No user SQL runs; an adapter runs only its own startup queries.
    pub async fn probe(source: &DataSource, resolver: &Resolver, fresh: bool)
        -> std::result::Result<(), ProbeFailure>
}
```

The body of `probe`:

1. `resolve_fresh` or `resolve`. On an error, `Stage::Secret` with
   `error.to_string()`.
2. `adapter::connect(source.backend, &dsn).await`. On an error, the stage
   comes from the match above. The message is `error.to_string()` with the
   resolved DSN replaced by its `mask_dsn` form. Skip the replacement when
   the DSN is empty.
3. Drop the adapter and return `Ok(())`.

`open_with` stays as it is. Its two lines are not worth sharing at the cost
of changing what `connect` prints.

`crates/binsql-core/src/lib.rs:29`: `pub use session::{ProbeFailure, Session, Stage};`.

### CLI

`crates/binsql/src/cli/mod.rs`:

- Move the `(None, None)` arm of `connect` (`:246-261`) into
  `pub fn default_source(config: &Workspace) -> Result<(String, DataSource)>`,
  keeping the messages and exits word for word. `connect` calls it.
  `source test` calls it, so both say the same thing.

`crates/binsql/src/cli/source.rs`:

- `SWITCHES` gains `"fresh"`. Read it once after `--scope`. If it is set and
  `operation != "test"`, exit 2 with `--fresh applies only to source test`.
- Arms in the existing match, each returning early because neither prints a
  source row:
  - `("test", [] | [_]) => return test(names.first(), fresh, &options).await`
  - `("test", _)`: exit 2, `source test takes at most one name`.
  - `("clear-cache", [])`: run
    `Resolver::from_env().cache().clear()`, turning an error into
    `failed(format!("clearing the secret cache: {error}"))`, then
    `return Ok(())`.
  - `("clear-cache", _)`: exit 2, `source clear-cache takes no name`.
- The no-op usage message becomes
  `source needs a command: list, show, default, test or clear-cache`.
- `async fn test(name: Option<&String>, fresh: bool, options: &Options) -> Result<()>`:
  1. `load()`.
  2. Get the id from `find` when there is a name, or from `default_source`
     when there is none.
  3. Get the source, then call
     `Session::probe(source, &Resolver::from_env(), fresh)` and time it.
  4. Build the row. On success it is `Text(id), Bool(ok), Null, Int(ms), Null`.
     On failure, `stage` and `error` are `Text`.
  5. Print it through `render::rows` with a `ResultSet` from a second small
     builder, `test_table`, with columns `name text, ok bool, stage text,
     elapsed_ms int, error text`. Model it on `table` (`:161`).
  6. On failure return
     `failed(format!("{id} failed at the {stage} stage: {message}"))`.
- Update the module doc (`:1-7`): `test` is the one op that resolves a secret
  and connects.

## Relevant lore

None found. The project note applies to the CLI tests: integration tests must
never reach `ratatui::init`. Every `source test` and `source clear-cache`
form returns before the TUI.

## Acceptance criteria

- `binsql source test NAME` on a good sqlite source prints one row with
  `ok: true`, `stage: null` and `error: null`, then exits 0.
- A sqlite source whose file is missing prints a row with `ok: false` and
  `stage: "connect"`, then exits 1 with an `error:` line on stderr.
- A source whose DSN is a bare vault URL fails at `stage: "secret"`, exit 1.
- A postgres literal with a password, pointed at a closed port, fails at
  `connect`. The password appears nowhere on stdout or stderr.
- `source test` with no name tests the default. With no default it is exit 2,
  with `connect`'s message.
- `resolve_fresh` does not return a cached value, and `resolve` still does.
- `source clear-cache` removes `secret-cache.json` and `cache.key` from the
  config directory, prints nothing on stdout, and exits 0. It also exits 0
  when neither file exists.
- These are exit 2:
  - `source test a b`
  - `source list --fresh`
  - `source clear-cache x`
- HELP's SOURCE section and the README's `### Data sources` cover `test`,
  `--fresh`, `clear-cache` and `BINSQL_SECRET_TTL=0`.

## Tasks

- [x] **Resolver.** `resolve_with`, `resolve`, and `resolve_fresh` in
  `secrets/mod.rs`. Add these tests to its module, using `resolver(&dir)`
  (`:116`):
  - Seed `keyvault://binsql-no-such-vault/dsn` with `cache().put`.
    `resolve` returns the cached value. `resolve_fresh` errs: it went to `az`,
    which is missing or cannot read a vault that does not exist. Assert
    `is_err()` only, never the text, because it depends on whether `az` is
    installed.
  - `resolve_fresh` on a literal passes it through.

  In `cache.rs`'s tests, if there is no such test yet: `put` creates both
  files, `clear` removes both, and a second `clear` is `Ok`.

  Verify with `cargo test -p binsql-core secrets` and clippy.
- [x] **Probe.** Add `Stage`, `ProbeFailure` and `Session::probe` in
  `session.rs`, and the exports in `lib.rs`. Add tests in
  `crates/binsql-core/tests/secret_reference.rs`, reusing its `source()`
  helper (`:17-25`) and resolver setup. Only sqlite DataSources go through
  that helper; build the postgres one by hand.
  - A bare vault URL fails with `Stage::Secret`.
  - A sqlite path in a missing directory fails with `Stage::Connect`.
  - A seeded sqlite file is `Ok`.
  - `postgres://app:hunter2@127.0.0.1:1/app` fails with `Stage::Connect`,
    and the message does not contain `hunter2`. The connection is refused at
    once, so no server is needed.

  Verify with `cargo test -p binsql-core` and clippy.
- [ ] **CLI.** Make these changes:
  - `default_source` in `cli/mod.rs`.
  - In `cli/source.rs`: `--fresh`, the `test` and `clear-cache` arms, `test`
    and `test_table`.

  Add these integration tests to `tests/command_mode.rs`, after `:1047`. They
  use `Fixture`, `seed()`, and a config written as `write_sources` (`:865`)
  writes it:
  - With a source pointing at `fixture.database()` after `seed()`,
    `source test that -o json` succeeds. Its row has `ok` true and `stage`
    null.
  - A sqlite source at a missing path is `failed()`. Its stdout row has
    `stage` `"connect"`, and `stderr_has("error:")`.
  - A source with DSN `https://kv-x.vault.azure.net/` is `failed()` with
    `stage` `"secret"`.
  - A postgres literal with `hunter2` at `127.0.0.1:1` is `failed()`, and
    neither stdout nor stderr contains `hunter2`.
  - With the seeded source as the config's `default`, bare `source test`
    succeeds and names it. With no `default`, it is `refused()`.
  - `source test a b`, `source list --fresh` and `source clear-cache x` are
    `refused()`.
  - With `secret-cache.json` and `cache.key` written into
    `fixture.directory`, which is the config directory because
    `BINSQL_CONFIG` points there, `source clear-cache` succeeds with empty
    stdout. Neither file exists afterwards, and running it again succeeds.

  Verify with `cargo test -p binsql --test command_mode` and clippy. Run the
  whole file, so `connect`'s existing default tests cover `default_source`.
- [ ] **Docs.** Add to the command list and the SOURCE section of HELP in
  `cli/mod.rs`:
  - `source test [NAME] [--fresh]`.
  - What the row says, the stages, and the exits.
  - `--fresh` skips the cached secret and refreshes it.
  - `source clear-cache` deletes the cached secrets.
  - `BINSQL_SECRET_TTL=0` keeps secrets out of the cache for every verb.

  Put the same in the README's `### Data sources` (`:367-389`), after the
  `source default` paragraph. In the Status bullet (`:608-612`), name `test`
  and `clear-cache` as working, and leave the "no `add` yet" clause as it is.

  Verify with `cargo test -p binsql` and clippy, then read both sections.

## Files

- `crates/binsql-core/src/secrets/mod.rs`: `resolve_with`, `resolve_fresh`
  and the tests. Reuses `Cache::get`/`put`, `Reference::parse` and
  `azure::fetch`.
- `crates/binsql-core/src/secrets/cache.rs`: a `clear` test, if one is
  missing.
- `crates/binsql-core/src/session.rs`: `Stage`, `ProbeFailure` and
  `Session::probe`. Reuses `adapter::connect` and `config::mask_dsn`.
- `crates/binsql-core/src/lib.rs`: the exports.
- `crates/binsql-core/tests/secret_reference.rs`: the probe tests.
- `crates/binsql/src/cli/mod.rs`:
  - `default_source`, taken out of `connect`.
  - HELP.
- `crates/binsql/src/cli/source.rs`:
  - The `fresh` switch.
  - The `test` and `clear-cache` arms, `test` and `test_table`.
  - The usage text and the module doc.
  - Reuses `find`, `load`, `failed`, `usage`, `print` and `render::rows`.
- `crates/binsql/tests/command_mode.rs`: the integration tests.
- `README.md`: `### Data sources` and the Status bullet.

## Verification

- Run these, and clippy must report no warnings:
  - `cargo test -p binsql-core secrets`
  - `cargo test -p binsql-core`
  - `cargo test -p binsql --test command_mode source`
  - `cargo clippy --workspace --all-targets`
- By hand, with a scratch `BINSQL_CONFIG`:
  - Run `binsql source test <sqlite>` and `-o json --pretty`. Then break the
    path and check the exit code (`echo $?`) is 1, the row is on stdout, and
    the `error:` line is on stderr.
  - On a machine with a Key Vault source, run `binsql source test <kv>`
    twice, then `--fresh`, and watch the `az` call return on `--fresh`.
  - Run `binsql source clear-cache`, check both files are gone from the
    config directory, and check that `binsql source test <kv>` fetches again.

Material uncertainties:

- A driver's own error text could quote a fragment of the DSN rather than the
  whole string. Masking replaces only the whole string. The postgres test
  checks the common case.
- `azure::fetch` has no timeout. The `resolve_fresh` test reaches `az` when
  it is installed, so its time depends on the machine. It still fails,
  because the vault does not exist.
