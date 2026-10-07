---
title: What structured stderr contract does a caller opt into?
kind: grilling
mode: hitl
status: open
blocked_by: [3]
claimed_by:
---

## Question

Choose the additive error contract: recommend `--error-format text|json`,
text by default, independent of `--format`, with unchanged exits 0/1/2.
Settle the JSON schema and revision policy, stable categories and phases,
optional native code/hints, and whether all notices become JSON records too.
Define behaviour for invalid flags and errors before full argument parsing;
never emit an error on result stdout. Include redaction of DSNs, credentials,
SQL literals and subprocess diagnostics, without removing useful remediation.
Do not infer native codes or retryability from formatted prose. Decide how to
represent unknown transaction outcome rather than claiming rollback succeeded.
This decision should yield a small implementation task, not implement it here.

## Context

Ticket 03, its primary-source links, `crates/binsql/src/cli/mod.rs:24`,
`cli/exec.rs`, `crates/binsql-core/src/error.rs`, and secret resolution.
Read 09's answer if available for authentication categories; otherwise leave
those details for a task blocked by 09. No new exit numbers or changed defaults.

## Answer
