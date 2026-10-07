---
title: What does an opt-in connection deadline bound?
kind: grilling
mode: hitl
status: resolved
blocked_by: [3, 9]
claimed_by:
---

## Question

Choose units, flag, disabled/zero meaning and scope of a caller-selected
connection budget. Recommend `--connect-timeout-ms N`, absent/zero preserving
current driver defaults. Include Workspace loading as appropriate, keychain,
Key Vault through az, fedauth, socket/authentication and startup metadata probes.
Decide what is bounded end to end versus delegated to driver settings, how
subprocesses stop, and how cleanup remains bounded. Timeout must have a distinct
structured category with exit 1; missing credentials must remain actionable.
Preserve the TUI's source and credential resolution rules. Cut implementation
only after the behaviour and 09's findings settle what can actually be promised.

## Context

Tickets 03 and 09; `crates/binsql/src/cli/mod.rs:176`,
`crates/binsql-core/src/session.rs:37`, adapter connect paths and secrets/azure.rs.
Ticket 10's error decision if resolved. This is a caller budget, not a new
credential chain or a new login workflow.

## Answer

Add an opt-in shared flag, `--connect-timeout-ms N` (or `BINSQL_CONNECT_TIMEOUT_MS`), that caps the whole of `cli::connect` from config load to the end of the adapter's startup probes; when absent or `0`, nothing changes. The cap is a wall-clock deadline in the CLI. Running out of time is exit 1 with its own `connect timeout:` message. Any `az` still running is killed and the process exits straight away.

Read from the code at `8d5d475` on 2026-10-07. Nothing was run. Every claim about what a driver, `az` or the OS does at runtime is marked unchecked.

### Contract

- **Flag:** `--connect-timeout-ms N`, a whole number of milliseconds. It joins `SHARED_VALUES` (`crates/binsql/src/cli/mod.rs:138`), so `query`, `exec` and `inspect` all take it.
  - Absent or `0` means no budget, which is exactly today's behaviour. This is the same rule as `--limit 0` (`cli/query.rs:21-27`).
  - A value that is not a number is exit 2: `--connect-timeout-ms wants a number of milliseconds, got X`. This matches `--limit` (`cli/query.rs:25`).
  - `BINSQL_CONNECT_TIMEOUT_MS` fills it in when the flag is missing, and the flag wins. That is the order `BINSQL_CONN`, `BINSQL_DSN` and `BINSQL_DRIVER` already follow (`cli/mod.rs:96-97`, `:173-185`). A bad value in the variable is exit 2 too, as a bad `BINSQL_DRIVER` is (`:186`).
  - Assumed, not asked: milliseconds, and an environment variable alongside the flag. Milliseconds let an agent set budgets under a second, and the variable matches every other connection setting. Removing the variable later costs nothing.
- **What the budget covers, end to end:** everything inside `cli::connect` (`cli/mod.rs:167-238`). That is:
  - `Workspace::load`. This is local and synchronous, so the clock counts it but cannot interrupt it.
  - Name resolution.
  - Turning the stored DSN into a connection string (`secrets/mod.rs:61-95`): a keychain read, a Key Vault cache hit, or `az keyvault secret show` (`secrets/azure.rs:33-44`).
  - `adapter::connect` (`adapter/mod.rs:78`). For SQL Server this includes `fedauth=` → `az account get-access-token` (`adapter/mssql.rs:597-598`, `:705-719`) and the TCP connect, TLS and login. For PostgreSQL and MySQL it includes the pool's first connection (`adapter/postgres.rs:44-48`).
  - The startup probes: `SELECT version()` and `current_database()` (`postgres.rs:50-57`), and `@@VERSION` and `DB_NAME()` (`mssql.rs:51-57`). Their failures are swallowed with `.ok()` today, but their time counts against the budget.
- **What the budget does not cover:**
  - Connections opened later: `Session::adapter_for` → `open_catalog` for `--catalog` (`session.rs:76-102`), and SQL Server's reconnect after a cancel (`mssql.rs:109-111`). These belong to 12's statement budget.
  - The drivers' own waits. Today's driver settings are left alone. sqlx's default pool acquire timeout and whatever tiberius does are unchecked. If one of them fails first, the failure stays today's connect error, not a timeout.
- **How it is enforced:** `tokio::time::timeout` around the body of `connect`.
  - The CLI resolves the connection string and connects as two steps, so the message can say which phase ran out: `resolving the connection string` (keychain or Key Vault) or `connecting` (fedauth token, socket, TLS, login, probes).
  - This needs one small core addition: a `Session` constructor that takes an already-resolved connection string. `Session::open_with` today does both steps (`session.rs:36-52`).
  - `Session::open` keeps its behaviour, so the TUI's data-source and credential resolution rules are unchanged.
- **Making the slow steps interruptible:**
  - Both `az` commands gain `.kill_on_drop(true)` (`secrets/azure.rs:33`, `adapter/mssql.rs:706`). Today they have neither a timeout nor a kill (09). When the deadline drops the future, tokio kills the direct child.
  - Unchecked: whether `az`'s launcher passes control to Python itself or starts it as a child. A grandchild may outlive the kill; that is not promised.
  - `keychain::get` blocks (`secrets/keychain.rs:59-68`, `keyring`), so `Resolver::resolve` runs it in `spawn_blocking` and the deadline can fire over it. When time runs out, that thread is abandoned. Unchecked: whether a macOS Keychain access prompt can appear for a command-mode caller.
- **Keeping cleanup bounded:** when time runs out, the partly opened pool or socket is dropped, not closed politely. `cli::main` prints the error and `main` calls `std::process::exit` (`crates/binsql/src/main.rs:41`), which does not wait for the runtime or its blocking threads. The command therefore returns about when the budget expires. "About" means scheduling slack, which was not measured.
- **The timeout failure:** exit 1 (`cli/mod.rs:80`), on stderr: `error: connect timeout: no connection to <name> within <N> ms, while <phase>`.
  - The `connect timeout:` prefix stays fixed, so a caller can tell it apart from an authentication or credential failure today.
  - Ticket 10 is still open (`10-structured-errors.md`). When it lands, its JSON record carries category `connect-timeout` and the phase above. The text stays as it is.
  - Assumed, not asked: a fixed text prefix rather than a new `Error` variant. `Failure` holds only a message and a usage flag (`cli/mod.rs:25-28`), and 10 owns the structured shape.
- **Missing credentials stay actionable:** any error that arrives before the deadline passes through unchanged. That includes the keychain's `NoEntry` hint (`keychain.rs:62-65`), 09's `az` hints (`secrets/azure.rs:70-98`) and fedauth's `az login` hint (`mssql.rs:742-746`). Only the timeout itself becomes the new message.
- **What stays the same:** no default budget, no retry, no change to verbs, exit codes or formats (03), and no new credential chain (09).

### Build tickets

#### 50 — `az` and keychain reads can be interrupted
- blocked_by: []
- Add `.kill_on_drop(true)` to the `az` commands in `secrets/azure.rs:33` and `adapter/mssql.rs:706`. Nothing else in either function changes.
- In `Resolver::resolve` (`secrets/mod.rs:65-67`), run `keychain::get` in `tokio::task::spawn_blocking` and turn a join error into `Error::config`.
- Tests:
  - A `#[cfg(unix)]` test puts a stub `az` on `PATH` that writes its pid and sleeps. It drops `azure::fetch` under a 100 ms `tokio::time::timeout` and asserts the pid is gone within 2 s.
  - The existing resolver tests still pass.
- No flags, so no change to README.md or HELP.

#### 51 — `--connect-timeout-ms` caps opening a connection
- blocked_by: [50]
- Add a `Session` constructor that takes a resolved connection string. `open_with` calls it, and its behaviour is unchanged.
- Add `connect-timeout-ms` to `SHARED_VALUES`. Parse the flag, or `BINSQL_CONNECT_TIMEOUT_MS` when the flag is missing: absent or `0` means none, anything else that is not a number is exit 2.
- When a budget is set, `cli::connect` wraps loading, resolving and connecting in one deadline and reports which phase ran out with the `connect timeout:` message above.
- Tests in `crates/binsql/tests/command_mode.rs`:
  - A local `TcpListener` that accepts and never replies, used as `--dsn postgres://…` with `--connect-timeout-ms 300`, gives exit 1, `connect timeout:` and `while connecting` on stderr, empty stdout, and finishes in under 3 s.
  - A stub `az` that sleeps, a `keyvault://` source and `BINSQL_SECRET_TTL=0` give `while resolving the connection string`.
  - `--connect-timeout-ms abc` and the same value in the variable are exit 2.
  - A run with `0`, and a run with no flag, against SQLite produce output identical to today's.
- Update HELP under CONNECTION (`cli/mod.rs:91-97`) and README.md's command-mode flags with the flag, the variable, the meaning of `0`, what is covered, and that timeouts from driver settings are left alone.

#### 52 — README says what the connect budget promises
- blocked_by: [51, 21]
- In the README's Key Vault and Azure AD sections, say that the budget covers the `az` secret and token calls and kills `az` when time runs out. Say too that connections opened later for `--catalog` are outside the budget (ticket 12).
- Docs only, so no tests.
