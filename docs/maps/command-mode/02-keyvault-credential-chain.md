---
title: How did v2 get Key Vault tokens without `az`, and what are v3's options in Rust?
kind: research
mode: afk
status: open
blocked_by: []
claimed_by:
---

## Question

v3 resolves `keyvault://` references only through the Azure CLI
(`secrets/azure.rs`). v2 also had managed identity and service principal
credentials, which a CI job needs because it has no `az login`. Find out:

1. What v2's credential chain was: the order, the environment variables it
   read (`AZURE_CLIENT_ID`, `AZURE_TENANT_ID`, `AZURE_CLIENT_SECRET`,
   federated token files, and so on), and the endpoints it called.
2. The options in Rust, with their cost: the `azure_identity` crate (its
   dependency weight, its async runtime fit with tokio, its maturity), or
   hand-rolled HTTP to IMDS, App Service, and the OAuth client-credentials and
   workload-identity endpoints. Check whether the crate already in the tree for
   HTTP, if any, makes hand-rolling cheap.
3. Whether the Azure AD token for `fedauth=` SQL Server connections
   (README, "Azure AD for SQL Server") goes through the same `az` path and
   would want the same chain.

Do not pick an option. Ticket 06 decides; this one lays out what it would be
deciding between.

## Context

- `git show b9d4e03^:_old/` and search it for the Key Vault and credential
  code, e.g. `git show b9d4e03 --stat | grep -i -E "vault|azure|secret|cred"`.
- `crates/binsql-core/src/secrets/azure.rs`, `secrets/mod.rs`, `Cargo.toml`
  files for HTTP and TLS dependencies.
- Wherever `fedauth` is handled in `crates/binsql-core/src/adapter`.

## Answer
