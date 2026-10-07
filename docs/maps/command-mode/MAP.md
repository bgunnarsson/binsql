---
title: Command mode is an agent's tool for finding, managing and querying data sources
date: 2026-10-07
status: active
---

## Destination

`binsql` without the TUI is a tool built mainly for agents. An agent can find
the data sources there are, add, edit and remove them, and connect to one,
from the command line alone. It finds and stores them exactly as the TUI
does: the user config, the nearest `.binsql.json`, folders and qualified
names, the keychain, and Key Vault references through `az`. The commands
themselves are designed fresh for agents, not carried over from v2. Beyond
managing data sources, command mode carries the ergonomics an agent leans on,
each chosen by research rather than guessed. Not part of it: anything with a
pane, new database drivers, new places to look data sources up, and breaking
changes to `query`, `exec` and `inspect`.

## Notes

- Agents are the main users. Where an agent and a person at a terminal want
  different things, the agent wins.
- Data sources are looked up exactly as the TUI does. The CLI only adds
  commands on top of that.
- Nearly every database the person uses keeps its connection string in Azure
  Key Vault. Key Vault working well for an agent is central to the map, not an
  edge case.
- The person prefers building over long upfront design talk: decide routine
  calls yourself and flag the gaps afterwards.
- `query` reads, `exec` writes, `inspect` describes. That split is a safety
  boundary (`crates/binsql/src/cli/mod.rs`), and new verbs must not blur it.
- Argument parsing stays hand-rolled in `crates/binsql/src/cli/args.rs`; it is
  deliberately small rather than a parser dependency. A ticket that finds it
  too small says so and makes the case.
- Stdout carries only the `--format` output. Notes go to stderr. Exit codes are
  0 success, 1 the database said no, 2 a usage mistake.
- Config writes go through `Workspace` (`crates/binsql-core/src/workspace.rs`),
  so user and project scope, owner-only writes and keychain moves behave the
  way they do behind `⌃N`.
- The README documents every flag. A task that adds one updates `README.md`
  and the `HELP` text in `cli/mod.rs`, and removes its line from Status once
  that gap is closed.

## Decisions so far

- [03](03-agent-ergonomics-survey.md): Prioritize data-source discovery, structured errors, bounded waits and richer schema context; add opt-in streaming, capability discovery and row assertions, and decide DDL and safe plans separately while preserving existing defaults and the query/exec/inspect split.
- [04](04-inspect-for-agents.md): Inspect JSON gives an object list or one object's five column fields, not a full schema; the CLI drops identity and catalog context, while relationships, indexes, data row counts and definitions need new core introspection.
- [05](05-verb-namespace.md): Spend one bare verb, `source`, on all data-source management (`binsql source <op>`); every other addition is a flag or lives under an existing verb; `source` dispatches to command mode only when a non-flag argument follows it, so every `binsql source …` that worked before still opens the TUI; refuse to save a new top-level data source named after a verb; and add `binsql -- <name>` to open any saved name in the TUI.
- [07](07-tui-data-source-seams.md): The core holds every storage step (Workspace scope, set, remove, keychain, masking, resolution), but validation and the save and delete sequences live in the app; move them into `binsql-core` before the CLI writes, and add the default setter and connection test, which nothing has today.
- [09](09-keyvault-for-agents.md): Key Vault through `az` never hangs on a prompt, but has no deadline, gives every failure the same exit 1 told apart only by text, cannot be checked without connecting or cleared from the command line, and does not work without `az login`; `az` stays the only mechanism and the README documents its non-interactive logins.
- [17](17-safe-plan-support.md): All four backends plan without executing, but `EXPLAIN ANALYZE <write>` passes today's guards and runs on PostgreSQL and MySQL; fix the classification (22), then add estimated plans only, as each backend's native payload, shaped in 23.
- [19](19-schema-context-contract.md): Inspect gains one opt-in switch, `--columns`: flat rows of every selected object's columns with catalog, schema, name and kind beside today's five fields, through the existing renderer; keys, relationships, indexes, generated columns and type limits are later additions.
- [08](08-data-source-commands.md): Settled `binsql source` as seven operations: `list`, `show`, `add`, `edit`, `remove`, `default` and `test`. The connection test reports whether it failed at the secret, at the Azure AD token, or at the database. Reading Key Vault with `--fresh` skips the cache, and `source clear-cache` clears it.
- [10](10-structured-errors.md): Callers who want machine-readable errors add `--error-format json` (or `BINSQL_ERROR_FORMAT=json`); each failure then prints as one JSON line on stderr. Text stays the default, the exit codes stay 0/1/2, and stdout still carries nothing but `--format` output.
- [11](11-connect-budget.md): Add an opt-in shared flag, `--connect-timeout-ms N` (or `BINSQL_CONNECT_TIMEOUT_MS`), that caps the whole of `cli::connect` from config load to the end of the adapter's startup probes; when absent or `0`, nothing changes. The cap is a wall-clock deadline in the CLI. Running out of time is exit 1 with its own `connect timeout:` message. Any `az` still running is killed and the process exits straight away.
- [12](12-statement-budget.md): `query`, `exec` and `inspect` get one additive flag, `--timeout-ms N`. It is off by default, and `0` also means off. It sets one client-side deadline that covers everything a command does after the connection opens, for the whole batch. When it fires, the command uses the existing cancel path and gets a fixed 2000 ms to clean up, then exits 1 with a timeout message that never claims a rollback.
- [13](13-streaming-contract.md): `binsql query` gets one opt-in switch, `--stream`. It is allowed only with `-o jsonl|csv|tsv` and only for a statement that does not write, even when `--allow-write` is given. It writes each row to stdout as the row arrives, through a bounded queue. A run that finishes without error prints exactly the bytes the buffered path prints for the same rows. Exit status stays the only signal that the output is complete. Truncation goes to stderr as a note, never into the stream.
- [16](16-ddl-output.md): Inspect gets one opt-in switch, `--definitions`, which returns each selected table's and view's own definition text as the server stores or prints it, for an agent to read, not to restore from. Where a backend has no native text for an object, binsql says so in the row and does not build a CREATE statement from the columns. No external dump tool is called.

- [18](18-verb-reservation.md): Built: a new top-level data source named `query`, `exec` or `inspect` is refused by `Workspace::set` (and before any secret is filed in the TUI), and `binsql -- <name>` opens any saved name.
- [20](20-core-save-delete.md): Built: binsql_core::source::Draft::build validates a data source with the form's rules and messages; Workspace::save and Workspace::delete run the save and delete sequences through a SecretStore trait (Keychain is the real one); the TUI calls them and keeps only its UI work.
- [21](21-keyvault-ci-docs.md): Built: README's Key Vault section names az login --service-principal (password or --federated-token) and --identity for CI, containers and agents, says fedauth= uses the same login, BINSQL_SECRET_TTL=0 picks up a rotated secret, and that the az call has no deadline yet; Status says those identities are reached through az.
- [22](22-explain-analyze-guard.md): Resolved: EXPLAIN, DESCRIBE and DESC that run their statement (ANALYZE/ANALYSE, bare or in PostgreSQL's option list, quoted or not) classify as that statement; a plain or ANALYZE-off EXPLAIN stays a read.
## Follow-up tickets from 03

- [10 — Structured errors](10-structured-errors.md): settle opt-in stderr records.
- [11 — Connect budget](11-connect-budget.md): settle the connection deadline after 09.
- [12 — Statement budget](12-statement-budget.md): settle execution and cleanup deadlines.
- [13 — Streaming contract](13-streaming-contract.md): settle export and partial-output semantics.
- [14 — Capability probe](14-capability-probe.md): build an offline JSON manifest.
- [15 — Require rows](15-require-rows.md): build a narrow opt-in query assertion.
- [16 — DDL output](16-ddl-output.md): settle definition fidelity after 04.
- [17 — Safe plan support](17-safe-plan-support.md): research backends and EXPLAIN classification.

## Follow-up tickets from 05

- [18 — Verb reservation](18-verb-reservation.md): build the reserved names and `binsql -- <name>`.

## Follow-up tickets from 04

- [19 — Schema context contract](19-schema-context-contract.md): settle additive one-call object/column context and the scope of new metadata acquisition.

## Follow-up tickets from 19

- [24 — Column order and nullability](24-column-order-nullability.md): order column queries, surface decode failures, fix SQL Server nullability.
- [25 — inspect --columns](25-inspect-columns.md): every object's columns with identity in one call.
- [26 — Exact name under --columns](26-inspect-columns-exact-name.md): one object by exact name, ambiguity is exit 2.

## Follow-up tickets from 07

- [20 — Core save and delete](20-core-save-delete.md): move validation and the save and delete sequences into `binsql-core`.

## Follow-up tickets from 09

- [21 — Key Vault for CI](21-keyvault-ci-docs.md): document non-interactive `az login` and forcing a fresh read.

## Follow-up tickets from 17

- [22 — EXPLAIN ANALYZE guard](22-explain-analyze-guard.md): classify an executing EXPLAIN as the statement it runs.
- [23 — Plan shape](23-plan-shape.md): settle where estimated plans live and what they return.

## Follow-up tickets from 08

- [30 — `binsql source` verb with `list` and `show`](30-binsql-source-verb-with-list-and-show.md)
- [31 — `source default` and `Workspace::set_default`](31-source-default-and-workspace-set-default.md)
- [32 — `source test`, `--fresh` and `clear-cache`](32-source-test-fresh-and-clear-cache.md)
- [33 — `source add` and `source edit`](33-source-add-and-source-edit.md)
- [34 — `source remove`](34-source-remove.md)

## Follow-up tickets from 10

- [40 — `--error-format json` gives one structured error record on stderr](40-error-format-json-gives-one-structured-error-recor.md)
- [41 — Key Vault and Azure AD failures carry a reason](41-key-vault-and-azure-ad-failures-carry-a-reason.md)
- [42 — Database errors carry the native code when the driver types it](42-database-errors-carry-the-native-code-when-the-dri.md)
- [43 — exec reports the actual transaction outcome](43-exec-reports-the-actual-transaction-outcome.md)

## Follow-up tickets from 11

- [50 — `az` and keychain reads can be interrupted](50-az-and-keychain-reads-can-be-interrupted.md)
- [51 — `--connect-timeout-ms` caps opening a connection](51-connect-timeout-ms-caps-opening-a-connection.md)
- [52 — README says what the connect budget promises](52-readme-says-what-the-connect-budget-promises.md)

## Follow-up tickets from 12

- [60 — `--timeout-ms` for query, with a bounded cancel](60-timeout-ms-for-query-with-a-bounded-cancel.md)
- [61 — exec under the deadline: whole batch, honest messages](61-exec-under-the-deadline-whole-batch-honest-message.md)
- [62 — inspect under the deadline](62-inspect-under-the-deadline.md)
- [63 — No COMMIT after a cancel](63-no-commit-after-a-cancel.md)
- [64 — PostgreSQL and MySQL stop the server statement inside a transaction](64-postgresql-and-mysql-stop-the-server-statement-ins.md)
- [65 — Check PostgreSQL behaviour under `--timeout-ms`](65-check-postgresql-behaviour-under-timeout-ms.md)
- [66 — Check MySQL behaviour under `--timeout-ms`](66-check-mysql-behaviour-under-timeout-ms.md)
- [67 — Check SQL Server behaviour under `--timeout-ms`](67-check-sql-server-behaviour-under-timeout-ms.md)
- [68 — Check SQLite behaviour under `--timeout-ms`](68-check-sqlite-behaviour-under-timeout-ms.md)

## Follow-up tickets from 13

- [70 — Core streams rows into a bounded sink](70-core-streams-rows-into-a-bounded-sink.md)
- [71 — Render encodes one record at a time without changing output](71-render-encodes-one-record-at-a-time-without-changi.md)
- [72 — `query --stream` writes jsonl/csv/tsv as rows arrive](72-query-stream-writes-jsonl-csv-tsv-as-rows-arrive.md)

## Follow-up tickets from 16

- [80 — Core reads object definitions, with SQLite native text](80-core-reads-object-definitions-with-sqlite-native-t.md)
- [81 — PostgreSQL views return their query](81-postgresql-views-return-their-query.md)
- [82 — MySQL returns `SHOW CREATE` text](82-mysql-returns-show-create-text.md)
- [83 — SQL Server views return their module text](83-sql-server-views-return-their-module-text.md)
- [84 — `inspect --definitions` prints one definition row per selected object](84-inspect-definitions-prints-one-definition-row-per.md)

## Not yet specified

- **README Status**: rewriting the section once the data-source commands (30–34) land.

## Out of scope

- TUI work. The map is about the command line; the grid, tree and history in
  the app are separate.
- New drivers. The four backends stay as they are.
- Basing anything on v2's `binsql conn`. The commands are designed fresh.
- Breaking changes to existing verbs, flags, exit codes or output formats.
  Scripts already depend on them. Additions only.

- Shell completions for this map: 03 found stronger agent needs in capability
  discovery and saved-source listing; no interactive shell requirement was
  established. Revisit in a separate map if requested.
