---
title: "Key Vault and Azure AD failures carry a reason"
date: 2026-10-07
status: active
---

## Context

Ticket [41](../maps/command-mode/41-key-vault-and-azure-ad-failures-carry-a-reason.md)
builds the second step of [10](../maps/command-mode/10-structured-errors.md)'s
contract. Today `secrets/azure.rs` reads `az`'s stderr in `explain()` and folds
a hint into an `Error::Config` string, and the `fedauth=` token failure is an
`Error::Connect` labelled `azure ad`, which `Session::probe` tells apart by
that label. Outcome: the JSON error record carries `reason`, `hint` and a
redacted, length-capped `detail` for both, and text output is unchanged.

Settled: unattended loop; work stays on `main`, each task committed and pushed;
no AI mentions in commits. Additions only: text output stays byte-for-byte the
same, and no exit code changes.

## Assumed, not asked

- `Error::Secret { reference, reason, hint, detail }` builds today's text from
  its parts, so nothing carries the message twice.
- Every failure to start `az` is `az-missing`, as today's text already says
  ("Is the Azure CLI installed?"); its hint is to install the Azure CLI.
- The token failure stays `Error::Connect`, which gains `reason`; `probe`
  reads the token stage from the reason instead of the `azure ad` label, so a
  connection string spelled `azure ad` can no longer pass for it.
- The token failure carries the reason and, when `az` ran and failed, its
  stderr as `detail`; it has no hint, since today's text has none.
- `detail` is capped at 1000 characters, cut on a character boundary and
  ended with `…`.
- JSON's message for a secret failure is the text up to the reference
  (`connecting to prod: reading keyvault://v/s`); the stderr is in `detail`
  and the next step in `hint`.
- `source test` keeps its stage-based category and gets no reason: its
  failure is a stage and a string, and its row already shows the stage.
- `az` failing to give a parseable or non-empty value stays `Error::Config`
  with no reason: none of 09's reasons fits it.

## Relevant lore

None in `docs/solutions`. From the map: tests never reach `ratatui::init`;
never name a zsh variable `status`.

## Acceptance criteria

- `explain()`'s substrings map to `az-unauthenticated`, `vault-forbidden`,
  `secret-not-found` and `vault-not-found`; an unmatched stderr has no reason
  and becomes the `detail`.
- `Error::Secret`'s text is exactly what `Error::Config` printed before for
  each case.
- In JSON, a secret failure is category `secret`, phase `connect`, with
  `reason`, `hint` and `detail` after `message`; the token failure is
  category `connect` with reason `azure-ad-token`.
- `detail` goes through the same masking as `message`: a JWT in it is masked.

## Tasks

- [ ] **1. The core types the failure.**
  `error.rs`: `Reason` with `as_str`; `Error::Secret`; `Error::Connect`
  gains `reason`, and `Error::token` builds the `azure ad` one.
  `secrets/azure.rs`: `classify` returns the reason and hint, `fetch`
  returns `Error::Secret`. `adapter/mssql.rs`: the token failures use
  `Error::token`. `session.rs`: `probe` reads the reason.
  Verify: `cargo test -p binsql-core` — one test per classification, the
  existing fixtures, and the text of each case unchanged.

- [ ] **2. The record carries them.**
  `cli/mod.rs`: `Failure` gains `reason`, `hint` and `detail`; `caused`
  fills them from the error and sets the JSON message; `error_record`
  writes them in the contract's order, `detail` masked and capped.
  Verify: `cargo test -p binsql` — a unit test that a JWT in `detail` is
  masked, and one of a secret failure's record.

- [ ] **3. HELP and the README.** The Structured errors section lists
  `reason`, `hint` and `detail`, and the reasons.
  Verify: `cargo test -p binsql`.
