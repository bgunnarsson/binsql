---
title: Where does Key Vault resolution get in an agent's way today?
kind: research
mode: afk
status: open
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
