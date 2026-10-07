---
title: "The README says what the connect budget promises"
date: 2026-10-07
status: active
---

## Context

Ticket [52](../maps/command-mode/52-readme-says-what-the-connect-budget-promises.md)
finishes 11's connect budget in the docs. 51 described the flag under Command
mode, but the Azure AD and Key Vault sections still say nothing of it, and the
Key Vault section ends saying there is no deadline on `az` and that `az`
shares binsql's stdin, both untrue since 50. Outcome: both sections say what
`--connect-timeout-ms` does to the `az` calls they describe.

Settled: unattended loop; work stays on `main`, each task committed and pushed;
no AI mentions in commits. Docs only, so no tests.

## Assumed, not asked

- `--catalog` needs a sentence in only one place: SQL Server reads every
  catalog over the one connection (`open_catalog` returns `None`), so no
  token is fetched for it, and no catalog connection re-reads Key Vault — it
  reuses the string the primary connection resolved. The Command mode
  paragraph already says such connections are outside the budget.
- The SQL Server reconnect after a cancelled statement fetches a new token
  outside the budget, and the Azure AD section says so.
- The stale closing sentence of the Key Vault section is replaced, not kept
  beside the new text: `az`'s stdin is closed, so it cannot stop at a prompt.

## Relevant lore

None in `docs/solutions`.

## Acceptance criteria

- The Azure AD section says fetching the token is part of connecting, that
  running out kills `az` and what it started, with `while connecting`, and
  that the reconnect after a cancel is outside the budget.
- The Key Vault section says the secret read counts, that running out kills
  `az` and caches nothing, with `while resolving the connection string`, that
  a cache hit makes no call, and that `--catalog` never reads the vault again.
- Nothing in the README still says `az` has no deadline or shares stdin.

## Tasks

- [ ] **1. The two sections.** `README.md`: Azure AD for SQL Server, Azure Key
  Vault references.
  Verify: `cargo test -p binsql` (the README is not tested, but HELP is
  unchanged).
