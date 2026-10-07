---
title: How does opt-in result streaming report partial output?
kind: grilling
mode: hitl
status: open
blocked_by: [3]
claimed_by:
---

## Question

Decide an opt-in query streaming contract for large exports: which formats,
flag, metadata channel and termination semantics. Recommend bounded buffering
for JSONL/CSV/TSV first, leaving existing buffered output unchanged. Preserve
row-only JSONL, existing JSON envelopes and limit defaults (absent/zero means
unlimited); do not append metadata rows to existing streams. Explain truncation,
errors after partial output, cancellation, broken pipe and success detection.
Settle interaction with query's existing --allow-write explicitly. A fetch cap
bounds retained rows but does not promise bounded server work or row byte size. Cut adapter streaming
and CLI writing into separate implementation tasks after this decision.

## Context

Ticket 03; `crates/binsql/src/cli/query.rs:61`, cli/render.rs:329,
cli/mod.rs:338; `crates/binsql-core/src/adapter/mod.rs`, adapter/sqlx_common.rs:104,
adapter/mssql.rs:158 and value.rs. See psql FETCH_COUNT and mysql --quick docs.
No changes to exec receipts or query safety permissions.

## Answer
