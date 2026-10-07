---
title: "README says what the connect budget promises"
kind: task
mode: afk
status: resolved
blocked_by: [51, 21]
claimed_by:
---

## Question

Build what 11 settled for this step; the contract is in 11's answer
(`11-connect-budget.md`).

- In the README's Key Vault and Azure AD sections, say that the budget covers the `az` secret and token calls and kills `az` when time runs out. Say too that connections opened later for `--catalog` are outside the budget (ticket 12).
- Docs only, so no tests.

## Answer

The Azure AD and Key Vault sections say that `--connect-timeout-ms` counts the
`az` token and secret calls, kills `az` and what it started when time runs
out, and that `--catalog` neither fetches a second token nor reads the vault
again; the stale "no deadline, shares stdin" sentence is gone.

Built in docs/plans/2026-10-07-connect-budget-docs.md (README.md). The
correctness review found nothing false. It noted that a reconnect reusing the
token fails once the token has expired, which is how the code already behaves
and is not a promise of the budget; accepted. Docs only, so no security review.

Assumed, not asked:
- `--catalog` needs a sentence only where it matters: SQL Server reads every catalog over the one connection, and catalog connections reuse the resolved string.
- The token is fetched once, baked into the tiberius `Config`, so the reconnect after a cancel calls no `az`.
- The stale closing sentence of the Key Vault section is replaced, not kept beside the new text.
