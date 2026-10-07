---
title: What do scripts and agents need from a SQL CLI that binsql's command mode lacks?
kind: research
mode: afk
status: resolved
blocked_by: []
claimed_by:
---

## Question

The destination asks for agent and script ergonomics chosen by research rather
than guessed. Survey what comparable command-line SQL tools offer to
non-interactive callers: `psql`, `sqlite3`, `usql`, `sq`, `duckdb`,
`mysql`/`mycli`, `sqlcmd`, and any CLI built for agents. Compare that with
binsql's command mode today. Cover at least:

- discovering what can be connected to (listing saved data sources);
- failures a program can parse, and exit-code conventions;
- statement and connect timeouts;
- schema dumps and DDL output;
- `EXPLAIN` / plans;
- shell completions;
- a `--version` or capability probe that an agent can parse;
- streaming large results, and `--limit` behaviour;
- a non-zero exit on an empty result, or on an assertion.

Return a ranked candidate list. For each candidate, say what it would add, what
it would cost, and whether it bends the query/exec/inspect safety split. Mark
the ones whose shape is the person's call rather than obvious. These become the
map's next tickets.

## Context

- `README.md` "Command mode", `crates/binsql/src/cli/` (all of it, about 1,850
  lines), and `HELP` in `cli/mod.rs`.
- Out of scope per `MAP.md`: TUI features, new drivers, breaking changes.

## Answer

Prioritize data-source discovery, structured errors, bounded waits and richer schema context; add opt-in streaming, capability discovery and row assertions, and decide DDL and safe plans separately while preserving existing defaults and the query/exec/inspect split.

### Evidence and limits

Surveyed the primary documentation below on 2026-10-07, and checked the current
Rust code at `bb0ecbf`. There were no resolved tickets to fold in: 01, 02 and
06 were dropped; 04, 05, 07, 08 and 09 remained open. Their questions are not
answers, so this survey leaves their decisions to them.

Ran `cargo test -p binsql --test command_mode`: all 12 integration tests passed.
Also ran `target/debug/binsql` against an isolated temporary SQLite database,
with `BINSQL_CONFIG` pointing at a temporary config, `BINSQL_PROJECT` empty,
and the connection environment variables removed:

| Probe | Observed |
| --- | --- |
| `--version` | Exit 0, stdout `binsql 3.0.0`, empty stderr. |
| `query 'SELECT 1 AS n WHERE 0' -o json` | Exit 0; `row_count: 0`, `rows: []`, `columns: []`, `truncated: false`. |
| Five-row recursive SELECT with `--limit 2 -o json` | Exit 0; two rows, `truncated: true`. |
| Missing table with `-o json` | Exit 1; empty stdout; plain-text `error: ...` on stderr. |
| `query 'SELECT 1' --timeout 1` | Exit 2; unknown option on stderr. |
| `query 'EXPLAIN QUERY PLAN SELECT 1' -o json` | Exit 0; SQLite's native plan rows. |
| Empty SELECT with `-o jsonl` | Exit 0; empty stdout and stderr. |

Unchecked: competitor binaries were not run; this is a documentation survey,
not a conformance test. PostgreSQL, MySQL and SQL Server runtime behaviour,
network and Key Vault timeout/cancellation behaviour, and memory under a large
export were not measured. Costs below are estimates from the interfaces, not
benchmarks. A format called JSONL does not establish streaming in any tool.

### What comparable tools establish

| Tool | Relevant documented interface | Implication for binsql |
| --- | --- | --- |
| `psql` | `-c`/`-f`, CSV, `-w` (no password prompt), `-X` (no startup script), `--version`, database listing and description commands. Exit 0 normally, 1 for its own fatal error, 2 for a broken noninteractive connection, 3 for script errors with `ON_ERROR_STOP`; SQLSTATE variables and diagnostic verbosity. `FETCH_COUNT` fetches and prints SELECT rows in groups, with possible errors after partial output. [Manual](https://www.postgresql.org/docs/current/app-psql.html). | Stable failure detection and bounded result memory matter more than interactive conveniences. Its exit numbers cannot replace binsql's existing 0/1/2 meanings. Database listing is not saved-source discovery. |
| PostgreSQL companion interfaces | libpq's `connect_timeout` is a connection parameter; server `statement_timeout` bounds execution. `pg_dump --schema-only` exports definitions. `EXPLAIN` supports JSON; `ANALYZE` actually executes the statement, including writes. [Connections](https://www.postgresql.org/docs/current/libpq-connect.html), [timeouts](https://www.postgresql.org/docs/18/runtime-config-client.html), [dump](https://www.postgresql.org/docs/18/app-pgdump.html), [plans](https://www.postgresql.org/docs/18/sql-explain.html). | Connection and statement budgets are distinct; metadata, native DDL and execution plans are distinct products too. |
| `sqlite3` | Batch input, JSON/CSV modes, `.bail`, `.schema`, `.dump`, `.eqp`, `.version`, and `.timeout MS`. The last is a lock/busy wait, not a total execution timeout. [CLI manual](https://www.sqlite.org/cli.html). | Single-call schema definitions and native plans are useful; copying the name “timeout” without its semantics is not. |
| `usql` | Named connections in YAML, command/file execution, JSON/CSV, `--no-password`, `--no-init`, version and interactive SQL completion. [README](https://github.com/xo/usql). | Saved names and deterministic noninteractive execution are established needs. SQL completion is not shell completion or agent discovery. |
| `sq` | `sq ls` lists saved source handles and groups; output formats apply to management and inspection too. JSON errors are configurable independently of result format, and ping and shell completion have their own budgets. [Sources](https://sq.io/docs/source/), [output](https://sq.io/docs/output/), [config](https://sq.io/docs/config). | A source inventory and a separate error-output contract are good precedents; a ping timeout does not prove a statement deadline. |
| `duckdb` | `-c`/`-f`, `-batch`, `-bail`, JSON/CSV, `-readonly`, version, schema and plan dot commands. `.maxrows` limits display in duckbox mode only; safe mode limits filesystem/external access. [Arguments](https://duckdb.org/docs/current/clients/cli/arguments), [dot commands](https://duckdb.org/docs/current/clients/cli/dot_commands), [safe mode](https://www.duckdb.org/docs/current/clients/cli/safe_mode). | A display cap is not a fetch cap; host-access safety is not the same boundary as read/write SQL. |
| `mysql` / `mycli` | mysql has batch output, `--connect-timeout`, `--quick` for row-at-a-time retrieval and `--force` to continue after errors. mycli exposes saved DSN listing, execute/batch formats (CSV/TSV/table), an unbuffered option and completions. [mysql options](https://dev.mysql.com/doc/refman/8.4/en/mysql-command-options.html), [mycli option definitions](https://raw.githubusercontent.com/dbcli/mycli/main/mycli/main.py). | Streaming needs an explicit implementation. mysql's “force” means something different from binsql's destructive-statement override; do not copy it. |
| `sqlcmd` | Separate `-l` login and `-t` query timeouts, `-b` for failure exit and `-V` for severity threshold. `:EXIT(SELECT ...)` turns a result into an exit status, with a defined no-row failure. Go and ODBC variants have differences. [Options](https://learn.microsoft.com/en-us/sql/tools/sqlcmd/sqlcmd-utility?view=sql-server-ver17), [commands](https://learn.microsoft.com/en-us/sql/tools/sqlcmd/sqlcmd-commands?view=sql-server-ver17). | CI assertions and distinct wait budgets have precedents, but arbitrary SQL result-to-exit mapping would break binsql's exit contract. |
| `agent-sql` | Describes itself as a CLI for AI agents; documents connection listing/testing, schema dump, plans, per-command timeout, limits, JSON errors and advisory records on stderr, and explicit write opt-in. [README](https://github.com/shhac/agent-sql). | Supports the same priorities. Its metadata records and defaults are its own contract; adding them to binsql's existing row-only JSONL or replacing binsql's default format/limit would break callers. |

These are positive examples, not a claim that every unmentioned feature is
absent from every tool. No surveyed source establishes a universal
`--fail-if-empty` convention. The narrow row assertion below is a recommendation
for binsql, inferred from CI use and sqlcmd's result-driven exits.

### Binsql today

- Saved-source lookup already uses `Workspace`, including user/project merging,
  default and qualified-name resolution (`crates/binsql/src/cli/mod.rs:176`,
  `crates/binsql-core/src/workspace.rs:19`). No inventory verb exists: the
  command namespace contains only query/exec/inspect (`cli/mod.rs:47`). Scope,
  masked DSN and connection tests belong to existing 07/08, not another design.
- Failures are reduced to a string and a usage boolean (`cli/mod.rs:24`), then
  printed as text, regardless of result format (`cli/mod.rs:71`). Exit 1 also
  covers connection, config, I/O and cancellation failures, not just a database
  rejecting SQL. `binsql-core/src/error.rs:6` retains some typed categories,
  but the CLI discards them. JSON errors need a separate opt-in flag, stable
  categories, safe messages and structured notices; changing `-o json` alone
  must not silently change stderr.
- No deadline flags exist in the shared flags (`cli/mod.rs:143`) or query flags
  (`cli/query.rs:15`). Cancellation is installed after connecting
  (`cli/query.rs:32`, `cli/query.rs:59`); inspect has no cancellation token
  (`cli/inspect.rs:15`). Opening includes secret resolution and driver setup
  (`binsql-core/src/session.rs:37`). Driver defaults may bound particular waits;
  binsql offers no portable caller-selected end-to-end bound.
- Inspect lists schema/name/kind or one table's column/type/nullable/default/PK
  (`cli/inspect.rs:79`, `cli/inspect.rs:129`), not native CREATE statements or
  all tables' details in one call. Ticket 04 owns the per-backend metadata gap
  audit. Schema context for writing SQL ranks above restoration-quality DDL.
- `--version` already works (`crates/binsql/src/main.rs:248`); do not ticket it
  as missing. Help lists flags (`cli/mod.rs:83`), but there is no structured
  capability manifest or shell-completion command in that namespace.
- `--limit` is unlimited when omitted or zero (`cli/query.rs:21`). The adapters
  collect up to N rows and observe an extra row to mark truncation
  (`binsql-core/src/adapter/sqlx_common.rs:166`, `adapter/mssql.rs:211`). This
  bounds collected rows but does not promise to bound server-side execution,
  execution time or row byte size; it does not rewrite SQL. Query then renders the complete `ResultSet` into a
  string (`cli/query.rs:65`, `cli/render.rs:71`); JSONL also builds a vector of
  JSON objects (`cli/render.rs:329`). None of these formats streams to stdout.
  JSON carries `row_count` and `truncated` (`cli/render.rs:301`); JSONL/CSV/TSV
  omit that metadata. Broken pipe is already success (`cli/mod.rs:338`).
- Zero rows are ordinary success (`cli/query.rs:65`); there is no row/assertion
  flag. SQL can express an assertion as `SELECT ... WHERE condition`, but a
  script currently has to parse its output to decide whether it passed.
- Native SQLite EXPLAIN works through query. The classifier labels any leading
  EXPLAIN as Read (`binsql-core/src/sql.rs:181`), without checking ANALYZE or
  its inner statement. PostgreSQL's documented execution behaviour exposes a
  safety gap by code inspection; a dedicated plan command must not perpetuate
  it. No live PostgreSQL write probe was run.

In references above, `cli/...` is under `crates/binsql/src/` and `adapter/...`
is under `crates/binsql-core/src/`.

### Ranked candidates

Costs are relative: small = CLI and docs; medium = retaining core metadata or
cancellation across layers; large = adapter interfaces and all four backends.
“Person's call” marks a product choice to grill, not an implementation detail.
All additions keep table output, unlimited default results, and exits 0/1/2.

| Rank | Candidate and what it adds | Cost and safety split | Next ticket / person's call |
| --- | --- | --- | --- |
| 1 | Saved-source list/show/test with qualified name, scope, driver, readonly/default state and safe DSN display; discovery must not resolve secrets or contact servers. | Small to medium, pending 07. Listing describes config; testing only connects. Writes remain management operations through Workspace. | Existing 07/08, namespace 05. Person's call in 08. |
| 2 | Opt-in JSON errors on stderr, with stable category, phase, message and optional backend code/hint; notices structured in the same mode. | Medium: preserve errors before formatting, audit redaction and transaction claims. No SQL authority change. Native error codes can be absent; do not derive them by parsing prose. | 10, grilling: exact schema and flag contract are a person's call. Recommend separate `--error-format json`, text default and unchanged exits. |
| 3 | Caller-selected connect budget covering secret resolution, socket/auth and startup probes, with a timeout distinct from authentication failure. | Medium; must stop subprocesses and bound cleanup, not just drop an await. No SQL authority change. Key Vault/fedauth path needs 09. | 11, grilling: units/default/zero and scope are a person's call. Recommend opt-in milliseconds, no new default. |
| 4 | Statement budget for query/exec and metadata reads, including cancellation and honest transaction outcome reporting. | Medium to large across adapter cancellation/rollback paths. Timeout never grants write permission; cancelling a write cannot promise nothing happened without evidence. | 12, grilling: per-statement versus whole-batch budget and cleanup policy are a person's call. |
| 5 | One-call structured schema context, including relationships/indexes where available, without running user SQL. | Medium, pending 04's backend inventory. Fits inspect. Native DDL is separate. | Existing 04 owns research; retain only its remaining build/shape fog. |
| 6 | Opt-in streaming for export formats, bounded buffering, early output and an explicit partial-result/truncation contract. | Large: adapters currently return collected ResultSet; writer needs backpressure, cancellation and broken-pipe handling. Keep it on query, and settle interaction with `--allow-write`; no new write permission. | 13, grilling. Person's call: formats and metadata channel. Existing JSONL remains row-only, JSON remains its current envelope, `--limit 0` stays unlimited. |
| 7 | Offline JSON capability probe for installed version, supported backends, verbs, flags/formats, exit meanings and contract revision. | Small, no config/secrets/server work. No SQL authority change, and a global flag spends no data-source name. | 14, afk task: add `--capabilities`; plain `--version` stays unchanged. Backend support means compiled support, not credentials or server reachability. |
| 8 | Opt-in `query --require-rows`: exit 1 when zero rows, otherwise ordinary success; use SQL predicates for assertions. | Small, retaining normal result stdout and an error category on stderr. Fits query; not an assertion DSL or affected-row check on writes. | 15, afk task. No person's call needed for this narrow opt-in; default empty success remains. `SELECT COUNT(*)` always returns a row, so callers must predicate the assertion. |
| 9 | Native DDL for an object or schema, with backend limitations stated instead of inventing a restorable script from column metadata. | Medium to large; SQLite sqlite_schema and MySQL SHOW CREATE differ from PostgreSQL/SQL Server scripting. Fits inspect; applying a dump remains exec. | 16, grilling after 04. Person's call: context-only definitions versus restoration fidelity, objects and unsupported cases. Recommend context-only first. |
| 10 | Portable estimated-plan interface with backend-native payloads and a separate explicit execution policy. | Medium to large, especially SQL Server session options. Describe plans under inspect or read-only query flag, never a silent execution mode. Existing EXPLAIN classification needs follow-up before claiming safety. | 17, research: establish backend support and classifier gap first; plan UX remains fog until then and 05. Person's call later: whether to expose runtime plans at all. |
| 11 | Shell completions for verbs, flags and saved names. | Small to medium maintenance of shell scripts/hand-rolled flag inventory; dynamic completion must not resolve secrets. No SQL authority change. | Defer outside this map: agents need an offline manifest and source inventory; no interactive shell requirement was established. SQL autocomplete is TUI work and already excluded. |

### Map consequences and notes for the next session

Create 10–17 as above, leaving their answers empty. Source commands are already
covered by 05/07/08; do not duplicate them. Keep the schema feature's shape/build
fog waiting on 04, and plan implementation waiting on 17. Do not add a default
row cap, timeout, automatic retry or machine-output format by analogy with
another CLI: each would change an existing contract. A transport timeout on a
write is not proof that retrying is safe.

Notes beyond this ticket's change: the isolated SQLite empty-query probe lost
its column metadata, consistent with columns being populated only when a row
arrives (`adapter/sqlx_common.rs:159`); 04's worker should consider that. The
EXPLAIN ANALYZE classifier concern above is recorded for 17, not repaired here.
Key Vault/auth remediation categories must be informed by 09 rather than guessed.
