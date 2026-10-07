---
title: Expose an offline JSON capability manifest
kind: task
mode: afk
status: resolved
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

`binsql --capabilities` prints one compact JSON object — version, backends, each command's options and switches, `source` operations, formats, error formats and exit codes — and exits 0 without reading the config.

Built in docs/plans/2026-10-07-capability-probe.md (cli/mod.rs, cli/inspect.rs,
cli/source.rs, cli/query.rs, cli/exec.rs, main.rs, tests/command_mode.rs, README, HELP). The security
review found nothing. The correctness review confirmed the lists match what
each parser accepts, and found two small gaps, both fixed: the README's list of
flags that print and stop now names `--capabilities`, and the plan now says
that, like `--version`, the flag works only before a verb.

Assumed, not asked:
- Shape `{"type":"capabilities","schema":1,…}`; `schema` rises only when a field is renamed or removed.
- Each command lists `options` and `switches` as typed, aliases as their own entries.
- `formats` lists canonical names; aliases stay accepted but unlisted.
- Compact JSON on one line; no `--pretty`.
- Like `--version`, it prints and stops before any verb; after a verb it is an unknown option.
