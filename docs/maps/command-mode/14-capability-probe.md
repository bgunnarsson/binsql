---
title: Expose an offline JSON capability manifest
kind: task
mode: afk
status: open
blocked_by: [3]
claimed_by:
---

## Question

Add a global `binsql --capabilities` flag that prints a JSON object and exits
0 before reading config, resolving secrets or connecting. Include a manifest
revision, installed package version, supported backends, command verbs, accepted
flags and output formats, and exit-code meanings. Describe compiled features,
not live server/authentication capabilities. Keep --version output and existing
commands unchanged; use no new bare verb. Derive shared lists from existing
constants where practical, retaining the hand-rolled parser. Document the flag
in README and help. Verify parseable output, offline operation even with invalid
config paths, agreement with accepted flags/formats and unchanged --version.

## Context

Ticket 03; `crates/binsql/src/main.rs:237` (global parsing) and main.rs:248 (version), cli/mod.rs HELP/VERBS/shared
flags, cli/args.rs, cli/render.rs Format, backend.rs and command_mode tests.
Re-read current command lists at implementation time so the manifest includes
features landed since the survey. No server probing or shell completions.

## Answer
