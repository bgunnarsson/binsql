---
title: "Key Vault and Azure AD failures carry a reason"
kind: task
mode: afk
status: resolved
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

A Key Vault failure is an `Error::Secret` and the Azure AD token failure a `Connect` with a reason; in JSON they carry `reason` (`az-missing`, `az-unauthenticated`, `vault-forbidden`, `secret-not-found`, `vault-not-found`, `azure-ad-token`), and a secret failure `hint` and a redacted, 1000-character `detail`.

Built in docs/plans/2026-10-07-secret-failure-reason.md (crates/binsql-core/src/error.rs: Reason, Error::Secret, Error::token; secrets/azure.rs: classify; adapter/mssql.rs; session.rs's probe; crates/binsql/src/cli/mod.rs: Failure's reason and parts, caused, error_record; README's Structured errors; HELP's ERRORS). Text output keeps its bytes and every exit code is unchanged. The correctness and security reviews found nothing to fix. Not fixed here: when a data source's connection string comes from a secret, the resolved string reaches an adapter's connect error and is masked only by the generic patterns, since the CLI holds only the reference.

Assumed, not asked: `Error::Secret { reference, reason, hint, detail }` builds today's text from its parts.
Assumed, not asked: every failure to start `az` is `az-missing`, with the hint to install the Azure CLI.
Assumed, not asked: the token failure stays `Error::Connect`, which gains `reason`; `probe` reads the token stage from it instead of the `azure ad` label.
Assumed, not asked: the token failure carries only the reason; its stderr is already in the redacted message.
Assumed, not asked: `detail` is redacted, then capped at 1000 characters on a character boundary and ended with `…`.
Assumed, not asked: JSON's message for a secret failure stops at the reference (`connecting to prod: reading keyvault://v/s`).
Assumed, not asked: `source test` gets no reason, and `az` giving an unparseable or empty value stays `Error::Config` with none.
