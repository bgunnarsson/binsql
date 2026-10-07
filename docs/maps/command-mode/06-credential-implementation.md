---
title: Which credentials does Key Vault resolution support, in what order, and built how?
kind: grilling
mode: hitl
status: dropped
blocked_by: [2]
claimed_by:
---

## Question

Using ticket 02's options, decide with the person:

- which credentials are supported (Azure CLI, managed identity, service
  principal with a secret, workload identity or federated token) and in what
  order they are tried;
- whether the chain is implicit, like `DefaultAzureCredential`, or selected
  explicitly through an environment variable, so a CI job's failure names the
  credential it meant to use;
- whether it is built on `azure_identity` or hand-rolled;
- whether `fedauth=` SQL Server tokens share the chain.

The answer settles enough for the build to be cut into task tickets.

## Context

- Ticket 02.
- `crates/binsql-core/src/secrets/azure.rs`, which turns `az` errors into a
  next step. Whatever replaces it should keep that quality.

## Answer

Dropped: its premise, ticket 02, was dropped; credentials beyond `az` are out of scope.
