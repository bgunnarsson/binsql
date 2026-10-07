---
title: binsql source list and source show print the saved data sources without a secret
date: 2026-10-07
status: in-progress
---

## Context

Ticket 30 of the command-mode map
(`docs/maps/command-mode/30-binsql-source-verb-with-list-and-show.md`). The
contract is 08's answer (Dispatch, Parsing, the `list`/`show` rows of Ops,
Rows, The DSN column) and 05's decisions 2 and 5. Map rules apply: stdout
carries only `--format` output, notes go to stderr, exit 0/1/2, the README and
HELP document every flag.

Settled by the lead (the option that changes least):

- `source` joins `RESERVED_NAMES` (`crates/binsql-core/src/workspace.rs:56`,
  `[&str; 4]`). Through `Workspace::check_new_id` (`:181`) this also refuses a
  new top-level `source` in `⌃N`, which is 05's decision 3.
- `main.rs:41` dispatches on a new `cli::is_command(&args)`: true for
  `query`/`exec`/`inspect` as today, and for `source` only when `args[1]`
  exists and does not start with `-`. Every other `binsql source …` keeps the
  TUI path byte for byte. So `binsql source -o json list` is still the TUI's
  "Unknown option"; that is the price of 05's decision 2, and it stays.
- Ops in this ticket: `list` and `show <name>`. Any other op, including the
  ones tickets 31-34 will add, is exit 2 `unknown source command X` until then.
- `show` resolves through `Config::resolve` (`config.rs:283`), as `--conn`
  does; unknown or ambiguous is exit 2 with the `--conn` message, `no saved
  data source named X` (`cli/mod.rs:199`).
- Columns, in order: `name` (qualified), `scope` (`user`/`project`, from
  `Workspace::scope_of`; its `label()` is prose, so the column has its own
  words), `driver` (the backend's config name), `dsn`, `readonly`,
  `open_on_start`, `default`, `description`, `shadowed`. Bools are
  `Value::Bool`, the rest `Value::Text`.
- `dsn`: a `keychain://` or `keyvault://` reference verbatim, anything else
  through `mask_dsn` (`config.rs:355`). Nothing resolves a secret.
- `default`: the merged `Config.default`, put through `Config::resolve`, is
  this name.
- `shadowed`: the leaf (the text after `/`, or the whole name) is reserved
  and `Config::resolve(leaf)` is this name. That one rule covers a top-level
  verb name and a unique folder leaf. Each shadowed row gets a stderr note
  through `cli::note` (`cli/mod.rs:357`, silent under `-o none`):
  `note: binsql <leaf> runs the command; open it with binsql -- <leaf>,
  binsql <qualified> or --conn <qualified>`, and for a `source` leaf
  `note: binsql source alone still opens <qualified>`. `show` prints the same
  note for its row: one function builds the row and its note for both ops.

Design decided here:

- The SQL Server and ODBC forms quote values: `Password={se;cret}`,
  `Password='a b'`, `Password="a b"`. `mask_keyword_password`
  (`config.rs:390`) ends a value at the first `;` or space, so today the tail
  after a quoted `;` or space prints. The contract says no unmasked literal
  prints in any format, so the masking learns quoting: a value that opens
  with `{` ends at the matching `}` (a doubled `}}` is a literal brace), and
  one that opens with `'` or `"` ends at the same quote. Unquoted values are
  masked as before.
- The TUI path is never started by a test. `Command::output()` leaves the
  developer's `/dev/tty` reachable, so a test that reaches `ratatui::init`
  with a resolvable target can take over the terminal and hang. Only forms
  that end before the UI starts run as integration tests (`binsql source`
  with nothing saved, `binsql source --help`); the rest are unit tests on
  `cli::is_command` and `parse_args`, and on `Config::resolve` in core.
- `cli/source.rs` takes the op as `rest[0]` (dispatch guarantees it is a
  word), then calls `Args::parse(rest[1..], &["format", "o"], &["pretty",
  "no-header", "no-footer"])` and `cli::output`. `--conn`, `--dsn`,
  `--catalog`, `--schema` are unknown options, exit 2. `list` takes no
  positional, `show` exactly one; anything else is exit 2.
- The row is built by hand as a `ResultSet`, the pattern of `inspect::list`
  (`cli/inspect.rs:75-107`), and printed with `render::rows`.

## Relevant lore

None found.

## Acceptance criteria

- `binsql source list` prints one row per data source in the merged view, in
  `Config::iter` order, with the nine columns above, in every `-o` format.
- `binsql source show <name>` prints one row; a bare leaf unique across
  folders finds its `folder/name`; an unknown or ambiguous name is exit 2.
- No DSN column ever shows a password from a literal, including SQL Server
  `key=value` with quoted values; references print as written.
- A shadowed row is `shadowed: true` and has its note on stderr; stdout is
  unchanged by the note.
- `binsql source` with nothing saved by that name exits 1 with "not a saved
  data source"; `binsql source --help` prints the main help and exits 0;
  `binsql source -d postgres` and a saved `source` still go to the TUI.
- `binsql source frob` and `binsql source list --conn x` exit 2.
- HELP has a SOURCE section and the README a section for the two ops.

## Tasks

- [ ] **Masking.** In `config.rs`, teach `mask_keyword_password` the quoted
  value forms and add unit tests beside `masks_keyword_password` (`:459`):
  SQL Server `Server=x;User Id=u;Password=secret` →
  `Server=x;User Id=u;Password=****`; `Pwd={se;cret};Database=d` →
  `Pwd=****;Database=d`; `Password='a b';x=1` and `Password="a b";x=1`;
  postgres keyword `host=h user=u password=secret`; postgres URL with a
  `?password=` parameter; mysql URL `mysql://u:secret@h:3306/db`;
  go-sql-driver `u:secret@tcp(h:3306)/db`; a sqlite path unchanged.
  Verify: `cargo test -p binsql-core`, `cargo clippy --workspace --all-targets`.
- [ ] **Dispatch.** Add `source` to `RESERVED_NAMES`; add `cli::is_command`
  and use it at `main.rs:41`; add `mod source` and the `"source"` arm in
  `cli::main` (`cli/mod.rs:59`); create `cli/source.rs` whose `run` parses as
  above and answers every op with `unknown source command X` (exit 2), or
  `source needs a command: list or show` with none. Update
  `the_verbs_are_the_reserved_names` (`cli/mod.rs:369`), which asserts
  `source` is not a verb. Unit tests: `is_command` is false for `[source]`,
  `[source, -d, postgres]`, `[source, --help]`, `[source, --, x]` and true for
  `[source, list]`; `parse_args` of the first two gives target `source`.
  Core tests (if not already there): `resolve("source")` finds a top-level
  `source` and a unique `folder/source`; `check_new_id` refuses a new
  top-level `source` and allows `folder/source`. Integration tests in
  `tests/command_mode.rs`: bare `binsql source` → `failed()` +
  "not a saved data source"; `binsql source --help` → exit 0 and the main
  HELP; `binsql source frob` → `refused()` + "unknown source command frob".
  Verify: `cargo test -p binsql -p binsql-core`, clippy.
- [ ] **list and show.** In `cli/source.rs`: `row(config, id, source)`
  returning the values and the optional note, `list` and `show` building the
  `ResultSet` and printing notes after the output. Integration tests with a
  user config holding a literal postgres URL with a password, a
  `keychain://` and a `keyvault://` reference, a folder entry, a top-level
  `query`, a unique `team/inspect`, a `source`, and a `default`; plus a
  `BINSQL_PROJECT` `.binsql.json` holding one entry so `scope` shows both:
  `list -o json` has every column and value (password masked, references
  verbatim, `default` and `shadowed` right); `list` as a table has the header
  and the masked DSN and never the password; the two shadow notes are on
  stderr and absent under `-o none`; `show <unique leaf>` gives the folder
  row; `show nope` and an ambiguous leaf are exit 2; `list extra`,
  `show` with no name and `list --conn x` are exit 2.
  Verify: `cargo test -p binsql`, clippy.
- [ ] **Docs.** A SOURCE section in `cli::HELP` (after INSPECT, `cli/mod.rs:120`)
  naming `source list`, `source show NAME`, the columns, that references print
  as written and literals masked, and that only `-o`, `--pretty`,
  `--no-header`, `--no-footer` apply; add `binsql source list -o json` to
  EXAMPLES. A `### Data sources` subsection at the end of the README's
  `## Command mode` (before `## Databases`, `README.md:367`) saying the same,
  with the shadowing note and that `binsql source` alone still opens a saved
  `source`. In `## Status` (`README.md:580`), the first bullet now says
  listing and showing work and adding, editing and removing from the command
  line have not landed; the map rewrites Status once 31-34 land.
  Verify: `cargo test -p binsql` (HELP is asserted by existing tests, if any),
  clippy, and read both sections.

## Files

- `crates/binsql-core/src/config.rs`: quote-aware `mask_keyword_password`;
  new `mask_dsn` tests in the existing test module.
- `crates/binsql-core/src/workspace.rs`: `RESERVED_NAMES` gains `source`;
  tests for `check_new_id` with `source` if none cover it.
- `crates/binsql/src/main.rs`: `cli::is_command(&args)` at `:41`; unit tests
  beside `a_double_dash_makes_a_verb_a_target`.
- `crates/binsql/src/cli/mod.rs`: `is_command`, `mod source`, the `"source"`
  arm, HELP SOURCE, the updated verb test.
- `crates/binsql/src/cli/source.rs` (new): parsing, `list`, `show`, the row
  and its note; reuses `Args::parse`, `output`, `print`, `note`, `usage`,
  `render::rows`, `Workspace::load`/`scope_of`, `Config::iter`/`resolve`/
  `get`, `mask_dsn`, and the `ResultSet` pattern of `inspect::list`.
- `crates/binsql/tests/command_mode.rs`: the integration tests above, using
  `Fixture`/`Run`; a config written by hand per test, as `write_config` does.
- `README.md`: the Data sources subsection and the Status bullet.

## Verification

- `cargo test -p binsql -p binsql-core` and `cargo clippy --workspace
  --all-targets` with no warnings.
- By hand, with a scratch `BINSQL_CONFIG`: `binsql source list`,
  `binsql source list -o json --pretty`, `binsql source show <leaf>`,
  `binsql source nope` (exit 2), `binsql source` (exit 1, today's message),
  `binsql source --help` (main help). Check no output, in any format, holds
  a password from the config.
