---
title: "Key Vault and Azure AD failures carry a reason"
kind: task
mode: afk
status: open
blocked_by: [40]
claimed_by:
---

## Question

Build what 10 settled for this step; the contract is in 10's answer
(`10-structured-errors.md`).

- `secrets/azure.rs` classifies `az`'s raw stderr where it already reads it (`explain()`, `:70-98`) into a typed reason: `az-missing`, `az-unauthenticated`, `vault-forbidden`, `secret-not-found`, `vault-not-found` (09). It returns a new `Error::Secret { reference, reason, hint, detail }`, whose text output is identical to today's message.
- The `fedauth=` token failure (`adapter/mssql.rs:705`, per 09) gets the reason `azure-ad-token` under category `connect`.
- The CLI maps these to category `secret` (or `connect`) with `reason`, `hint`, and a redacted, length-capped `detail`. If no substring matches, there is no `reason` and the stderr goes into `detail`.
- Tests: unit tests for each classification using the existing stderr fixtures (`azure.rs:104-130`); a test that a JWT in `detail` is masked; a test that text output is unchanged.

## Answer
