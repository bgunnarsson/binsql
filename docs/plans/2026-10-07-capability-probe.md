---
title: "binsql --capabilities prints an offline JSON manifest"
date: 2026-10-07
status: active
---

## Context

Ticket [14](../maps/command-mode/14-capability-probe.md) adds a global
`--capabilities` flag. Today an agent learns what an installed binsql can do
by parsing `--help`, which is prose. Outcome: `binsql --capabilities` prints one
JSON object describing the compiled binary (version, backends, commands and
their flags, formats, exit codes) and exits 0, without reading config,
resolving a secret or connecting.

Settled: unattended loop; work stays on `main`, each task committed and pushed;
no AI mentions in commits. Additions only: `--version`, `--help` and every
command keep their output.

## Assumed, not asked

- The object's shape:
  `{"type":"capabilities","schema":1,"version":…,"backends":[…],"commands":{…},"formats":[…],"error_formats":["text","json"],"error_schema":1,"exit_codes":{"0":…,"1":…,"2":…}}`.
  `schema` is the manifest's revision, kept like the error record's: adding
  a field keeps it, renaming or removing one raises it.
- Each command lists `options` (flags that take a value) and `switches`, each
  spelled as typed: `--conn`, `-c`. Aliases are listed as their own entries,
  not grouped, since the parser's lists do not pair them.
- `source` also lists its `operations`, from a new constant; a test runs
  each one to show the dispatch knows it.
- `formats` lists the canonical names from `Format::NAMES`, not the aliases
  (`ndjson`, `md`, …), which stay accepted.
- Compact JSON on one line, as the error record is; `--capabilities` takes no
  `--pretty`.
- `--capabilities` behaves like `--version`: it prints and stops wherever it
  comes before `--`.
- `inspect`'s inline flag lists become constants like the other verbs'.

## Relevant lore

None in `docs/solutions`. From the map: tests never reach `ratatui::init`;
never name a zsh variable `status`; every new field or category is in HELP and
the README.

## Acceptance criteria

- `binsql --capabilities` prints one parseable JSON object and exits 0, with
  `BINSQL_CONFIG` pointing at a file that is not valid JSON.
- Every backend in `Backend::ALL`, every verb in `RESERVED_NAMES` and every
  format `Format::parse` accepts by its canonical name appears.
- Every option and switch listed for a command is accepted by that command's
  parser, and nothing the parser accepts is missing.
- `--version` prints exactly `binsql <version>`, as before.

## Tasks

- [x] **1. The manifest.** `cli/mod.rs`: `capabilities()` builds the object
  from `SHARED_*`, each verb's `VALUES`/`SWITCHES`, `source::OPERATIONS`,
  `Backend::ALL`, `RESERVED_NAMES` and `Format::NAMES`. `main.rs`:
  `--capabilities` prints it and stops.
  Verify: `cargo test -p binsql` — a unit test that each listed flag parses
  for its verb, an integration test with a broken config, and `--version`
  unchanged.

- [x] **2. HELP and the README.** The flag in `main.rs`'s OPTIONS and in the
  README's command-mode section, with the manifest's fields.
  Verify: `cargo test -p binsql`.
