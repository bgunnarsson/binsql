---
title: Document Key Vault for non-interactive callers in the README
kind: task
mode: afk
status: resolved
blocked_by: []
claimed_by:
---

## Question

09 kept `az` as the only Key Vault mechanism. Make that usable from CI and
containers through documentation only:

- In README "Azure Key Vault references" (`README.md:371`), name the
  non-interactive `az login` forms — `--service-principal`,
  `--federated-token`, `--identity` — as the way to give an agent or CI job
  a login, and say `fedauth=` uses the same login.
- Say that `BINSQL_SECRET_TTL=0` forces a fresh read, for a secret rotated
  within the cache lifetime.
- Say that `az` is run with no stdin, so a missing login fails rather than
  waits for a prompt, and that there is no deadline on it yet (11).
- Adjust the matching line in Status (`README.md:519`) if it describes CI as
  unsupported, to say what works now.

No code changes.

## Context

- 09's answer (`09-keyvault-for-agents.md`).

## Answer

Built: README's Key Vault section names az login --service-principal (password or --federated-token) and --identity for CI, containers and agents, says fedauth= uses the same login, BINSQL_SECRET_TTL=0 picks up a rotated secret, and that the az call has no deadline yet; Status says those identities are reached through az.

Departure from the question: it asked to say az runs with no stdin. Review and the verifier found that false — tokio's Command::output pipes only stdout and stderr, so az inherits binsql's stdin (secrets/azure.rs:33, adapter/mssql.rs:706). The README now says az shares binsql's stdin. Closing az's stdin in code (Stdio::null) is a proposal for the user, not built. The service-principal example uses --password="..." so a secret starting with - works, as Microsoft documents.
