---
title: What fidelity should inspect DDL promise?
kind: grilling
mode: hitl
status: resolved
blocked_by: [3, 4]
claimed_by:
---

## Question

Choose the objects and fidelity for additive native DDL output: one object
or a selected schema, context for an agent versus a restorable export, ordering,
format and treatment of unsupported objects/backends. Recommend context-only
native definitions first, clearly marking omissions rather than synthesizing
CREATE statements from columns. Keep this under inspect; applying SQL remains
exec. Decide whether calling an external dump utility is acceptable before
assuming pg_dump or a SQL Server scripting dependency. Cut backend acquisition
research/tasks only for the scope selected here.

## Context

Tickets 03 and 04; inspect.rs, core schema.rs and the four adapters.
Primary sources in 03: sqlite3 .schema, pg_dump --schema-only and SQL-native
metadata interfaces. Existing structured inspect formats remain unchanged.

## Answer

Inspect gets one opt-in switch, `--definitions`, which returns each selected table's and view's own definition text as the server stores or prints it, for an agent to read, not to restore from. Where a backend has no native text for an object, binsql says so in the row and does not build a CREATE statement from the columns. No external dump tool is called.

### Scope and flag

- **The flag:** `--definitions` is a switch added to the parse call at `crates/binsql/src/cli/inspect.rs:15`. Without it, `inspect` behaves byte for byte as today.
- **Selection:** it selects objects exactly as `--columns` does (19). That means one catalog (`inspect.rs:25`), `--schema` or every schema (`inspect.rs:45`), and an optional single positional name. The name is matched exactly and case-sensitively, is never split on dots, and a name found in several schemas exits 2 (26).
- **Objects covered:** only what inspect lists, which is tables and views (`schema.rs:6`, `adapter/mod.rs:33`). Standalone indexes, triggers, sequences, routines and constraints are not separate rows. They appear only where the backend's own table text already contains them.
- **Not with `--columns`:** giving `--definitions` and `--columns` together exits 2. Each switch has its own row shape, as 19 intended.
- **Writes:** none. Inspect describes; applying any of this text is still `exec`'s job.
- Assumed, not asked: the switch is named `--definitions`, not `--ddl` — PostgreSQL views return only their query, which is not DDL, and the plural matches `--columns`.
- Assumed, not asked: no `pg_dump`, `mysqldump`, SMO or `mssql-scripter`. Each is an extra install, version-coupled to the server, and needs its own credentials outside binsql's lookup and Key Vault path. That breaks the map's rule that data sources are looked up exactly as the TUI does.
- Assumed, not asked: the output is context, not a restorable export. A restorable dump needs dependency order, grants, sequences and object ownership; that is a separate map if asked for.

### Output, every format

One `ResultSet` through `render::rows`, the same envelope and formats as 19. The columns, in order:

`catalog` text, `schema` text, `object` text, `kind` text, `form` text, `definition` text

- **`catalog`, `schema`, `object`, `kind`:** the same meanings and null rules as 19.
- **`form`** is never null. It is one of:
  - `create`: a complete CREATE statement as the backend stores or prints it.
  - `query`: a view's SELECT body only, with no CREATE wrapper (PostgreSQL).
  - `unsupported`: binsql does not get a definition for this backend and kind. `definition` is null.
  - `withheld`: the backend has the object but returned no text (an encrypted module, a missing permission, or SQLite's null `sql`). `definition` is null.
- **`definition`:** the text exactly as returned. It is not reformatted, re-quoted or trimmed, and binsql never adds a CREATE wrapper.
- **Ordering:** by `schema` (null first), then `object`, then `kind`, comparing exact bytes, as in 19. This is not dependency order.
- **Exit codes:**
  - **0:** complete, including rows marked `unsupported` or `withheld`. One stderr line counts those rows by form.
  - **1:** any acquisition error. It is all or nothing: stdout stays empty and stderr names the object.
  - **2:** a usage mistake (`--columns` alongside, two positionals) or an ambiguous name.
- **Cost:** one metadata statement per object and no data scans. Speed on large schemas is unchecked.

### What each backend gives

| Backend | Table | View | Source |
| --- | --- | --- | --- |
| SQLite | `create` | `create` | `SELECT sql FROM <catalog>.sqlite_master WHERE name = ? AND type IN ('table','view')`, catalog quoted in as at `adapter/sqlite.rs:98`. Null `sql` → `withheld`. |
| PostgreSQL | `unsupported` | `query` | `pg_get_viewdef(c.oid, true)` for relkind `v` and `m`; kinds `r`, `p` and `f` → `unsupported`. Looked up by `pg_namespace` + `pg_class` from the `ObjectRef` (which holds no oid, `schema.rs:32`). |
| MySQL | `create` | `create` | `SHOW CREATE TABLE` / `SHOW CREATE VIEW` on the identifier quoted by `ObjectRef::qualified` (`schema.rs:59`). |
| SQL Server | `unsupported` | `create` | `sys.sql_modules.definition` joined on `sys.objects`/`sys.schemas`. A null result (encrypted, or no VIEW DEFINITION permission) → `withheld`. |

- The text is native and nothing is synthesized. This follows ticket 16's own recommendation: neither PostgreSQL nor SQL Server has a server function that returns a table's CREATE text; that text exists only in their client dump tools, which are excluded above.
- The README spells out what this text does and does not contain:
  - SQLite table text has its inline constraints but not separately created indexes or triggers.
  - MySQL table text includes indexes, foreign keys, the `AUTO_INCREMENT=` counter and the view `DEFINER`.
  - SQLite text is stored as written, and may have been rewritten by `ALTER TABLE … RENAME`.
- Unchecked:
  - whether sqlx runs MySQL's `SHOW CREATE` through its prepared-statement path (fall back to a raw, unprepared query if it does not);
  - live PostgreSQL, MySQL and SQL Server output, and how permissions vary;
  - PostgreSQL's `pg_get_viewdef` on materialized views across server versions;
  - SQL Server synonyms and system-versioned tables;
  - whether `withheld` versus an error is told apart correctly on every server version.

### Core shape

- `schema.rs` gains:
  - `pub enum DefinitionForm { Create, Query, Unsupported, Withheld }`;
  - `pub struct Definition { pub form: DefinitionForm, pub text: Option<String> }`.
- The `Adapter` trait gains `async fn definition(&self, object: &ObjectRef) -> Result<Definition>` beside `columns` (`adapter/mod.rs:36`).
- `Session::definition` hands the call to the adapter for the object's catalog, exactly like `Session::columns` (`session.rs:215`), so PostgreSQL's second connection for another catalog works unchanged.

### Build tickets

- **80 — Core reads object definitions, with SQLite native text.** blocked_by: [20]
  - Adds `Definition`, `DefinitionForm`, `Adapter::definition` and `Session::definition`.
  - SQLite implements it from `<catalog>.sqlite_master.sql`. PostgreSQL, MySQL and SQL Server return `Unsupported` until 81–83 land.
  - Tests, against SQLite fixtures:
    - a table and a view, both `Create`, with the exact stored text;
    - an attached second catalog;
    - a table renamed with `ALTER TABLE … RENAME`, checking the stored text is returned untouched;
    - a missing object returns an error, not `Withheld`.
  - No flag, no README or HELP change.
- **81 — PostgreSQL views return their query.** blocked_by: [80]
  - Uses `pg_get_viewdef(oid, true)` for relkind `v` and `m`, giving `Query`. Kinds `r`, `p` and `f` give `Unsupported`.
  - Tests: a unit test of the relkind-to-form mapping; a live view and materialized view test only if a PostgreSQL test harness exists (unchecked).
  - Updates the README's per-backend row once 84 has added it.
- **82 — MySQL returns `SHOW CREATE` text.** blocked_by: [80]
  - Uses `SHOW CREATE TABLE` / `SHOW CREATE VIEW` on the quoted identifier, giving `Create`. Reads the `Create Table` or `Create View` column.
  - Tests: a unit test quoting a name that contains a backtick and a dot; a unit test choosing the result column by object kind.
  - Updates the README's per-backend row once 84 has added it.
- **83 — SQL Server views return their module text.** blocked_by: [80]
  - Uses `sys.sql_modules.definition` for views: present gives `Create`, null gives `Withheld`. Tables give `Unsupported`.
  - Tests: a unit test of the null-to-`Withheld` mapping and of the kind mapping.
  - Updates the README's per-backend row once 84 has added it.
- **84 — `inspect --definitions` prints one definition row per selected object.** blocked_by: [26, 80]
  - Adds the switch.
  - Reuses 25 and 26's selection, sort and exact-name matching.
  - Builds the row shape above, the stderr count of `unsupported` and `withheld` rows, all-or-nothing failure, and exit 2 when given with `--columns`.
  - Tests, against a SQLite fixture:
    - JSON with the exact envelope and rows, plus JSONL and CSV;
    - an exact name;
    - `--definitions --columns` exits 2 with empty stdout;
    - a unit test of the stderr note for `unsupported` rows;
    - `inspect`, `inspect <t>` and `inspect --columns` print byte-identical output to before.
  - README: the inspect section, a per-backend coverage table (rows for 81–83 marked unsupported until they land), the context-not-restore caveat, and the Status line. HELP: `--definitions` in `cli/mod.rs`.
