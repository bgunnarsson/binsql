---
title: What additive one-call schema context should inspect expose?
kind: research
mode: afk
status: claimed
blocked_by: [3, 4, 5]
claimed_by: lead
---

## Question

Settle the contract for an agent to get every selected object's columns and
identity in one inspect call, without changing existing inspect invocations or
formats. Choose the opt-in flag, scope (selected catalog/schema/object), output
structure and supported formats, deterministic ordering, identity selection
and failure/completeness reporting. Account for duplicate names across schemas,
case-sensitive names and dots in identifiers rather than silently selecting
the first match.

Decide the minimum context this feature promises: all available column fields
and catalog/schema/name/kind, then whether ordered primary keys, foreign keys
and indexes belong in its first contract or separate additions. Define unknown,
unsupported and absent metadata distinctly. Decide how to expose backend type
limits and generated/hidden columns. Recommend avoiding automatic data scans;
if counts are chosen, specify estimated versus exact and their cost explicitly.
Native DDL belongs to 16, not this response's fidelity promise.

Write only this contract, with evidence from 04 and primary metadata docs where
new acquisition is selected. Then cut small acquisition/build tickets for the
selected scope; each must preserve existing output, follow inspect's describe
boundary, and update README.md and CLI HELP for new flags. Do not implement it
in this research ticket.

## Context

- Resolved tickets 03, 04 and 05, and MAP.md's no-breaking-changes rule.
- `crates/binsql/src/cli/inspect.rs`, `cli/render.rs`, `cli/args.rs`, `cli/mod.rs`.
- `crates/binsql-core/src/schema.rs`, `value.rs`, `session.rs`, `adapter/mod.rs`
  and the four adapters; 04 distinguishes retained metadata from acquisition gaps.
- Existing 12 owns metadata deadlines; 16 owns native DDL. Avoid duplicating them.
- The map permits routine decisions without upfront grilling; flag any product
  choices that need overruling, and mark untested backend behavior unchecked.

## Answer
