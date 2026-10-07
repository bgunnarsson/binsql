---
title: What commands does the CLI offer for data sources, shaped for an agent?
kind: grilling
mode: hitl
status: resolved
blocked_by: [5, 7]
claimed_by:
---

## Question

With the namespace settled (05) and the core's seams known (07), decide with
the person what the data-source commands are and how they behave for an
agent:

- which operations exist: list, show, add, edit, remove, set the default,
  test the connection;
- what each prints in each `--format`, and what it never prints, such as a
  plain-text DSN;
- how a connection string reaches `add` without landing in the agent's
  transcript or the shell history (stdin, an environment variable, or an
  interactive prompt only);
- how user and project scope are chosen, and what the default is when a
  `.binsql.json` is in play;
- whether writes need a confirming flag, given that the caller is an agent.

The answer should settle enough to cut the build into task tickets.

## Context

- Tickets 05 and 07, and 03 if it is resolved.
- `crates/binsql/src/cli/mod.rs` and `render.rs`, for the output conventions
  already in place.

## Answer

Settled `binsql source` as seven operations: `list`, `show`, `add`, `edit`, `remove`, `default` and `test`. The connection test reports whether it failed at the secret, at the Azure AD token, or at the database. Reading Key Vault with `--fresh` skips the cache, and `source clear-cache` clears it.

`binsql source <op>` manages data sources through `Workspace`. A secret's value is never printed and never taken from the command line. Writes take no confirming flag except `remove --force`.

### Contract

**Dispatch.** `source` joins the verb list and the core's reserved names (ticket 18). It runs in command mode only when the next argument exists and does not start with `-` (05's decision 2, `main.rs:40`). Bare `binsql source`, `binsql source -d postgres` and `binsql source --help` keep today's TUI path, with the same message and exit code. An unknown op is exit 2.

**Parsing.** Source ops call `Args::parse` (`cli/args.rs:25`) with their own flag lists. They do not go through `cli::parse` (`cli/mod.rs:143`). They share only `-o/--format`, `--pretty`, `--no-header` and `--no-footer`. `--conn`, `--catalog` and `--schema` mean nothing here and are refused as unknown options (exit 2). `--dsn` is the shared spelling, with its own rule under `add` below.

**Ops.**

| Op | What it does | Exit |
| --- | --- | --- |
| `source list` | Every source in the merged view (`Config::iter`, `config.rs:186`), one row each. | 0 |
| `source show <name>` | One row. The name resolves through `Config::resolve`, as `--conn` does (`cli/mod.rs:192`). | 2 if unknown or ambiguous |
| `source add <name>` | Saves a new source. | 2 if the name exists (use `edit`), if validation fails, or if the name is reserved |
| `source edit <name>` | Changes only the fields given. `--rename NEW` renames and carries the keychain secret across. `--scope` moves the source between files. | 2 if unknown |
| `source remove <name> --force` | Removes the source, then its keychain secret once the config is written. | 2 without `--force` |
| `source default [<name>]` | With no name, prints the current default. With a name, sets it. | 2 if unknown |
| `source test [<name>]` | Opens a connection and runs no user SQL. With no name it tests the default, as `connect` does (`cli/mod.rs:216`). | 0 ok, 1 failed |
| `source clear-cache` | Clears the Key Vault secret cache (`Cache::clear`, `secrets/cache.rs:114`) and prints nothing on stdout. | 0 |

**Rows.** `list`, `show`, `add`, `edit`, `default` and `remove` all print the affected source's row through the existing renderer, so every `--format` works. `default` with no name prints the default's row; with no default set it prints nothing and exits 0. The columns are:

- `name`, the qualified name
- `scope`, `user` or `project` (`Workspace::scope_of`, `workspace.rs:155`)
- `driver`, `dsn`, `readonly`, `open_on_start`, `default`, `description`
- `shadowed`, which is true when a verb takes the bare name (05's decision 5); the note on how to open it goes to stderr

Unchecked: how a CLI-built row enters `render.rs`. Ticket 30 checks it before building.

**The DSN column.**
- A `keychain://` or `keyvault://` reference prints as written; it holds no secret.
- A literal prints through `mask_dsn` (`config.rs:355`).
- No op resolves a secret, except `test`.
- No op prints a resolved secret or an unmasked literal, in any format.
- Unchecked: `mask_dsn` on every driver's literal form, especially SQL Server `key=value` strings. Ticket 30 tests each one.

**How the connection string reaches `add` and `edit`.**
- `--dsn-stdin` reads one value from stdin and trims the trailing newline.
- `--dsn-env VAR` reads the variable that `VAR` names.
- `--dsn VALUE` on the command line is accepted only for a reference, or for the sqlite driver, where the DSN is a file path; neither holds a secret. Any other literal on the command line is exit 2, pointing at the two flags above.
- With no DSN given, `add` is exit 2. There is no interactive prompt.
- Assumed, not asked: no hidden-input prompt. An agent cannot answer one, and it would need a new terminal dependency. It can be added later without changing anything above.

**What else `add` and `edit` take.**
- `-d/--driver`, inferred when it is left out.
- `--description TEXT`.
- `--readonly` and `--no-readonly`.
- `--open-on-start` and `--no-open-on-start`.
- `--scope user|project`. Left out, it falls back to `Workspace::default_scope` (`workspace.rs:164`): back where the source came from, and a new one into the project when a `.binsql.json` is in play, as `⌃N` does.

Validation is ticket 20's core function, so `add` refuses exactly what `⌃N` refuses.

**Where a literal DSN goes.** It goes to the keychain by default: the config holds `keychain://<id>` (`keychain::reference`, `keychain.rs:55`) and the secret is stored through ticket 20's save.
- `--no-keychain` writes the literal into the file, and only with user scope. With project scope it is exit 2, because the project file is meant to be committed (`workspace.rs:8`).
- Assumed, not asked: literals are refused in the project file. It is shared and committed, and this is the stricter choice, easy to relax later.

**Setting the default.** `Workspace::set_default(id, scope)` writes `default` into the source's own file, or into the file `--scope` names. The project file's `default` wins the merge (`workspace.rs:132`). So writing the user file while the project file sets a default would have no effect, and that case is exit 1 with a message saying to pass `--scope project`.
- Assumed, not asked: refuse rather than quietly write the project file. A shared file is not changed without being asked.

**Confirmation.** `add`, `edit` and `default` need no flag: each can be undone by another command. `remove` needs `--force`, the word `exec` already uses for destructive statements (`cli/mod.rs:117`), because it deletes a keychain secret that nothing can restore.
- Assumed, not asked: only `remove` asks for confirmation. An agent is the caller, and an extra flag on undoable writes only costs turns.

**Connection test.** `test` runs in three stages, splitting `Session::open_with` (`session.rs:36`). It prints one row: `name`, `ok`, `stage`, `elapsed_ms` and `error`. `error` is the failure text, never the secret.

| `stage` | What it covers | Kind of failure |
| --- | --- | --- |
| `secret` | `Resolver::resolve` (`secrets/mod.rs:61`): keychain or Key Vault | Credential |
| `token` | The `fedauth=` `az` call, `Error::Connect{name:"azure ad"}` | Credential |
| `connect` | `adapter::connect` (`session.rs:45`) | Database |

Exit codes: 0 when everything passes. 1 on any failure; the row still goes to stdout, and an `error:` line goes to stderr.

Unchecked:
- Whether the `fedauth` token error can be told apart reliably, from `mssql.rs:705`.
- That `connect` runs only the adapters' startup metadata queries, never user SQL (ticket 07, point 5).

**Bypassing and clearing the cache.**
- `source test --fresh` skips the cache read and rewrites the cache entry on success. It needs a new `Resolver::resolve_fresh`.
- `source clear-cache` deletes the cache file and its key.
- `BINSQL_SECRET_TTL=0` (`secrets/mod.rs:35`) stays the way to bypass the cache on any verb. HELP and the README now say so.

Assumed, not asked: these names — `clear-cache` as an op rather than a flag, and `--fresh`. Both are additions and easy to rename before release.

### Build tickets

**30 — `binsql source` verb with `list` and `show`**
- blocked_by: [18]
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

**31 — `source default` and `Workspace::set_default`**
- blocked_by: [30]
- Write `default` into the source's file or the `--scope` file. Refuse with exit 1 when the project file's default would win.
- Tests in the core for user and project scope and the shadowing refusal. A CLI test that `source default` with no name prints the current default.
- HELP and README.

**32 — `source test`, `--fresh` and `clear-cache`**
- blocked_by: [30]
- A core probe that returns a `secret`, `token` or `connect` stage on failure. `Resolver::resolve_fresh`. `source clear-cache` calls `Cache::clear`.
- Tests:
  - The `secret` stage fails on a missing keychain entry or a bare vault URL (`secrets/mod.rs:73`).
  - A bad sqlite path fails the `connect` stage.
  - A good sqlite source is ok with exit 0.
  - `--fresh` skips a cached entry (using `Resolver::new` with a temporary cache directory).
  - `clear-cache` removes both files.
- HELP and README cover `test`, `--fresh`, `clear-cache` and `BINSQL_SECRET_TTL=0`.

**33 — `source add` and `source edit`**
- blocked_by: [20, 30]
- Input through `--dsn-stdin`, `--dsn-env` and `--dsn` (reference or sqlite only).
- Flags: `--driver`, `--description`, the readonly and open-on-start pairs, `--scope`, `--rename` and `--no-keychain`.
- Keychain by default through ticket 20's save. Print the saved row.
- Tests:
  - A literal on `--dsn` is exit 2.
  - `add` over an existing name is exit 2.
  - A reserved name is exit 2.
  - A literal with `--no-keychain` and project scope is exit 2.
  - `--scope` moves the source.
  - `edit` keeps the fields not given.
  - Validation errors match ticket 20's function.
  - Keychain writes go through ticket 20's test seam, never the real store.
- HELP and README.

**34 — `source remove`**
- blocked_by: [20, 30]
- Requires `--force`, else exit 2. Uses ticket 20's delete: config first, then the secret. Print the removed row.
- Tests: no `--force` is exit 2; an unknown name is exit 2; the source is gone from whichever file held it.
- HELP and README.
