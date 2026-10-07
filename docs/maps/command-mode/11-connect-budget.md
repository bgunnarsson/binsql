---
title: What does an opt-in connection deadline bound?
kind: grilling
mode: hitl
status: open
blocked_by: [3, 9]
claimed_by:
---

## Question

Choose units, flag, disabled/zero meaning and scope of a caller-selected
connection budget. Recommend `--connect-timeout-ms N`, absent/zero preserving
current driver defaults. Include Workspace loading as appropriate, keychain,
Key Vault through az, fedauth, socket/authentication and startup metadata probes.
Decide what is bounded end to end versus delegated to driver settings, how
subprocesses stop, and how cleanup remains bounded. Timeout must have a distinct
structured category with exit 1; missing credentials must remain actionable.
Preserve the TUI's source and credential resolution rules. Cut implementation
only after the behaviour and 09's findings settle what can actually be promised.

## Context

Tickets 03 and 09; `crates/binsql/src/cli/mod.rs:176`,
`crates/binsql-core/src/session.rs:37`, adapter connect paths and secrets/azure.rs.
Ticket 10's error decision if resolved. This is a caller budget, not a new
credential chain or a new login workflow.

## Answer
