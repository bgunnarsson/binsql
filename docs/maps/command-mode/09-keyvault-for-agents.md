---
title: Where does Key Vault resolution get in an agent's way today?
kind: research
mode: afk
status: resolved
blocked_by: []
claimed_by:
---

## Question

Nearly every database in use keeps its connection string in Azure Key Vault,
so an agent's first contact with a data source is usually a `keyvault://`
reference resolved through `az`. Trace that path in command mode end to end
and find where it fails or slows an agent down:

- `az` missing, not logged in, an expired token, the wrong tenant or
  subscription, or no access to the vault: what each one prints, its exit
  code, and whether an agent can tell them apart and know the next step;
- anything interactive that would hang a non-interactive caller, such as a
  device-code or browser prompt;
- latency: the cost of an `az` call per command, and how the 15-minute
  encrypted cache helps when an agent runs many commands in a row;
- whether an agent can check that a reference resolves without connecting or
  running SQL, and whether it can clear the cache or bypass it;
- the `fedauth=` Azure AD token path for SQL Server, which also shells out
  to `az`.

Also note what changes when the agent runs somewhere `az login` is not
available, such as CI or a container, so the fog item on credentials beyond
`az` can become a decision.

## Context

- `crates/binsql-core/src/secrets/` (`azure.rs`, `cache.rs`, `reference.rs`,
  `mod.rs`).
- `fedauth` handling under `crates/binsql-core/src/adapter/`.
- `crates/binsql/src/cli/mod.rs`, `connect`.
- README: "Azure Key Vault references", "Azure AD for SQL Server", "The
  schema cache".
- Tickets 02 and 06 were dropped when the map stopped following v2. This
  ticket starts from v3's code, not from them.

## Answer

The path works for an agent at a terminal where `az login` has been done, and never hangs on a prompt, but it has no deadline, every Key Vault failure is the same exit 1 told apart only by a hint in the text, a reference cannot be checked without connecting, the cache cannot be cleared from the command line, and nothing works where `az login` cannot run.

Read from v3's code on 2026-10-07. Key Vault round-trip latency was not
measured: no vault was reachable from this session. `az version` took 0.175 s
locally, and the Azure CLI's own start-up is the floor for every uncached call.

### The path

`cli::connect` (`crates/binsql/src/cli/mod.rs:167`) loads the `Workspace`,
resolves the name, then `Session::open` (`:235`) calls
`Resolver::from_env().resolve(dsn)` (`crates/binsql-core/src/secrets/mod.rs:61`):

1. a keychain account is read from the local store, uncached;
2. a literal DSN passes through, except a bare vault URL with no secret,
   which is refused with the `keyvault://<host>/<secret-name>` form to use;
3. a reference is parsed, looked up in the encrypted cache, and on a miss
   fetched by `azure::fetch` running
   `az keyvault secret show --vault-name V --name S [--version X] --output json`
   (`secrets/azure.rs`), then written to the cache. A failed cache write is
   ignored.

SQL Server's `fedauth=` is a second, separate `az` call,
`az account get-access-token --resource https://database.windows.net/`
(`crates/binsql-core/src/adapter/mssql.rs:705`), made on every connect with
no cache.

### Failures

All are on stderr as text, with exit 1. None is exit 2, so an agent cannot
use the exit code to tell a missing tool from a missing permission.

| Case | What it prints | Error |
| --- | --- | --- |
| `az` not installed | `running \`az\`: <os error>. Is the Azure CLI installed?` | `Error::Config` |
| Not logged in, expired or revoked token (`az login`, `please run`, `refresh token`, `aadsts` in stderr) | `reading <reference>: <az stderr>` + "no usable Azure credential — run `az login`" | `Error::Config` |
| Wrong subscription (`no subscription`) | same hint as not logged in | `Error::Config` |
| No access to the vault (`forbidden`, `does not have secrets get permission`) | az stderr + the Key Vault Secrets User role hint | `Error::Config` |
| Secret or version missing (`secretnotfound`, `was not found`) | az stderr + check the name and vault | `Error::Config` |
| Vault name wrong (`failed to resolve`, `name or service not known`) | az stderr + check the vault name | `Error::Config` |
| Wrong tenant | whatever az prints; an `AADSTS` code gets the `az login` hint, which is the right next step only with `--tenant` | `Error::Config` |
| `fedauth=` token failure | ``\`az account get-access-token\` failed: <stderr>`` or "`az` returned no accessToken. Try `az login`." | `Error::Connect{name: "azure ad"}` |

The hints come from substring matches in `explain()` (`secrets/azure.rs:70–98`);
az's own stderr is passed through verbatim, so the cause is there for an
agent that reads text. A machine-readable kind (not installed, not
authenticated, forbidden, not found) is what ticket 10's structured errors
would carry.

### Hangs and latency

- **No interactive prompt.** `Command::output()` sets stdin to null, and
  `az keyvault secret show` and `az account get-access-token` never start a
  device-code or browser flow on their own; an expired login fails rather
  than waits.
- **No deadline.** Neither `az` call has a timeout or `kill_on_drop`, so a
  stalled network or a slow token refresh holds the command for as long as
  az takes. That belongs in 11's connect budget, which must cover the
  secret fetch and the fedauth token, not only the driver's connect.
- **Latency.** The uncached cost is az's start-up plus one Key Vault round
  trip per reference. The cache is a file (`secret-cache.json`, AES-256-GCM,
  key in `cache.key`, both `0600`, `secrets/cache.rs`), so it spans
  processes: an agent running many commands in a row pays once per reference
  per 15 minutes (`BINSQL_SECRET_TTL`). `fedauth=` pays az on every command,
  because tokens are not cached.

### Checking and clearing

- There is no way to resolve a reference without connecting: `inspect` and
  `query` both open a session. A `binsql source test`-style command (07
  lists a connection test as missing) or a resolve-only flag would let an
  agent tell a credential problem from a database one before running SQL.
- `Cache::clear` exists but only tests call it. The only bypass is
  `BINSQL_SECRET_TTL=0`, which resolves every time and writes nothing. A
  rotated secret is therefore served from cache for up to 15 minutes; an
  agent that knows to set the variable can force a fresh read, but nothing
  says so in `--help`.
- A corrupt cache is a miss, so it never blocks a command.

### Where `az login` is not available

v3 resolves Key Vault through `az` only (README "Azure Key Vault references":
"Narrower than v2"). In CI or a container that has the Azure CLI, a service
principal (`az login --service-principal`), a federated token
(`az login --federated-token`) or a managed identity (`az login --identity`)
gives az a login, and binsql then works unchanged; with no Azure CLI at all,
every `keyvault://` source and every `fedauth=` source fails at the first
row of the table. The person's agents run on their own machine, where az is
logged in, so this map keeps `az` as the only mechanism and documents the
non-interactive `az login` forms for CI rather than linking an Azure SDK.

Assumed, not asked: credentials beyond `az` stay out of this map, and the
README gains the non-interactive `az login` forms — the person's agents run
where `az login` is done, linking an Azure identity stack is what v3 chose
not to do, and documenting the CLI's own forms is the change that is
smallest and easiest to undo.

### What follows

- 11's connect budget covers the `az` calls (secret and fedauth token) and
  kills `az` when it expires.
- 10's structured errors give Key Vault failures a kind: `az-missing`,
  `az-unauthenticated`, `vault-forbidden`, `secret-not-found`,
  `vault-not-found`, from the same substrings `explain()` already matches.
- A command to resolve a source without running SQL, and a way to bypass or
  clear the secret cache from the command line, belong with 08's data-source
  commands.
- A docs task: the README names the non-interactive `az login` forms and
  `BINSQL_SECRET_TTL=0` as the way to force a fresh read.
