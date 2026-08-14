# BINSQL

`BINSQL` is a terminal UI for exploring SQL databases. It supports SQLite, PostgreSQL, SQL Server (including Azure AD auth via Azure CLI), and MySQL from a single binary.

The goal is a fast, keyboard‑driven way to inspect schemas and data without leaving the terminal.

---

## Features

- Full‑screen terminal UI (TUI) with:
  - **Tables pane** (list of tables)
  - **Results grid** (auto‑sized columns, zebra striping)
  - **Query editor**
  - **Status bar**
- Row detail view (expand the currently selected row)
- Built‑in help overlay (`Ctrl+/` or `Ctrl+?`)
- Vim‑style pane navigation with `Ctrl+h/j/k/l`
- Driver‑aware connection header (`BINSQL SQLITE`, `BINSQL POSTGRES`, etc.)
- Driver‑agnostic core with per‑database adapters
- Support for:
  - **SQLite**
  - **PostgreSQL**
  - **SQL Server** (including Azure AD via `fedauth=ActiveDirectoryAzCli`)
  - **MySQL**
- A scriptable **command mode** for automation and AI coding agents:
  - `query` / `exec` / `tables` / `describe` / `schema` / `count` / `head`
  - Output as table, JSON, JSONL, CSV, TSV, markdown or vertical
  - Driver‑agnostic `?` bind parameters
  - Transactional multi‑statement scripts, with `--dry-run`
  - Saved connection profiles, including read‑only ones
  - **Azure Key Vault** connection strings, so no credential is stored on disk
  - Guardrails against unqualified `DELETE`/`UPDATE`, `DROP` and `TRUNCATE`

The UI uses a Catppuccin‑inspired dark theme; colors are chosen to sit nicely on typical dark terminals.

---

## Installation

### Prerequisites

- Go 1.22+
- For SQL Server with Azure AD via Azure CLI:
  - Azure CLI (`az`) installed and on `PATH`
  - Logged in with `az login`
- Network access to your databases

### Build from source

Clone the repository and build:

```bash
go build -o binsql ./cmd/binsql
```

Or use the existing build script (if present):

```bash
chmod +x scripts/build.sh
./scripts/build.sh
```

This can produce platform‑specific binaries in `./dist` (names like `binsql-darwin-arm64`, `binsql-linux-amd64`, etc.).

---

## Usage

`binsql` has two invocation styles:

```bash
binsql <command> [flags] [arguments]        # command mode
binsql [flags] <driver> <database-or-dsn>   # TUI / legacy form
```

They are told apart by the first positional argument: a driver name
(`sqlite`, `postgres`, `mssql`, `mysql`) selects the original behaviour,
anything else is treated as a command.

**Command mode** is the scriptable interface — see
[Command mode](#command-mode) below. It covers queries, writes, migrations
and schema inspection, with machine‑readable output.

**The legacy form** is unchanged:

- First argument: **driver**
- Second argument: **database path or DSN** (driver‑specific)
- If `-q` is omitted and stdout is a TTY → interactive **TUI**
- If `-q` is provided or stdout is not a TTY → **non‑interactive**; prints a single result table and exits

### Drivers

#### SQLite

Path to a `.sqlite` / `.db` file.

Interactive:

```bash
binsql sqlite ./cms.data.sqlite
```

Non‑interactive:

```bash
binsql -q "select * from languages limit 10" sqlite ./cms.data.sqlite
```

#### PostgreSQL

Use a standard PostgreSQL URL (pgx).

Interactive:

```bash
binsql postgres "postgres://user:pass@localhost:5432/mydb?sslmode=disable"
```

Non‑interactive:

```bash
binsql -q "select * from public.languages limit 10"   postgres "postgres://user:pass@localhost:5432/mydb?sslmode=disable"
```

When no query is provided in non‑interactive mode, a driver‑specific “list tables” query is run.

#### SQL Server (MSSQL)

Uses `github.com/microsoft/go-mssqldb` and the Azure AD driver wrapper for `fedauth` flows.

Basic SQL auth example:

```bash
binsql mssql "sqlserver://user:pass@sql-server:1433?database=MyDb&encrypt=disable"
```

##### Azure AD via Azure CLI (recommended for dev)

1. Log in with Azure CLI:

   ```bash
   az login
   ```

2. Run `binsql` with `fedauth=ActiveDirectoryAzCli`:

   ```bash
   PYTHONWARNINGS=ignore binsql mssql "server=xxx;database=xxx;encrypt=true;fedauth=ActiveDirectoryAzCli"
   ```

Notes:

- The MSSQL adapter detects `fedauth=` in the connection string and switches to the Azure AD driver.
- `PYTHONWARNINGS=ignore` works around Azure CLI Python warnings that can break `AzureCLICredential` on macOS.

You can also use other Azure AD flows supported by the driver (for example `fedauth=ActiveDirectoryInteractive` with `applicationclientid=`), as long as the DSN is accepted by `go-mssqldb`.

#### MySQL

Use the DSN format of `github.com/go-sql-driver/mysql`:

```bash
binsql mysql "user:pass@tcp(localhost:3306)/mydb?parseTime=true&charset=utf8mb4"
```

---

## Interactive TUI

When you start `binsql` without `-q`, you get a full‑screen interface built with [`tview`](https://github.com/rivo/tview) and [`tcell`](https://github.com/gdamore/tcell).

### Layout

The screen is split into four main areas:

- **Connection header** (top‑left)
  - Shows `BINSQL <DRIVER>` (for example `BINSQL SQLITE`, `BINSQL POSTGRES`).
- **Tables pane** (left column)
  - Lists tables for the current database.
- **Results grid** (main area)
  - Box‑drawing table with auto‑sized columns and zebra striping.
- **Query input + Status bar** (bottom)
  - Query box with prompt (`>`) and a status line with messages like:
    - `Tables loaded. Use arrows + Enter, or type a query below.`
    - `Query OK (42 rows, 3ms)`

### Pane behaviour

#### Tables pane

- Press **Enter** on a table name to run:

  ```sql
  SELECT * FROM <table> LIMIT 100;
  ```

- The query is also written into the query input box so you can tweak it.

#### Results grid

- Arrow keys move the selection between cells.
- Press **Enter** to open a **Row detail** overlay for the currently selected row:
  - One column per section (name + value).
  - Good for long text, JSON, or GUIDs that are truncated in the grid.

#### Query input

- Type any SQL and press **Enter** to run it.
- Results appear in the grid, and the status bar shows row count + execution time.

### Global keybindings

These work from anywhere in the main screen:

- **Ctrl+Q** / **Ctrl+C** – quit
- **Ctrl+R** – reload tables list
- **Ctrl+/** / **Ctrl+?** – toggle help overlay
- **Ctrl+:** – focus the query input from anywhere

Vim‑style pane navigation:

- **Ctrl+h** – focus **Tables** (left)
- **Ctrl+l** – focus **Results** (right)
- **Ctrl+j** – focus **Query** (down)
- **Ctrl+k** – focus **Status** (up)

### Overlays

Two overlays exist: **Row detail** and **Help**.

- Close overlays with:
  - **Esc**, **Enter**, **Ctrl+Q**, or **Ctrl+/**

#### Row detail

Opened with **Enter** while the results grid is focused.

- Shows each column as:

  ```text
  columnName:
    value
  ```

- Uses a scrollable text view, so long values are easy to read.

#### Help screen

Opened with **Ctrl+/** (or `Ctrl+?` on keyboards where that’s the same key).

It lists:

- Global shortcuts
- Pane‑specific behaviour
- Notes about mouse support (scroll + click)

Close with **Esc**, **Enter**, **Ctrl+Q**, or **Ctrl+/**.

---

## Non‑interactive mode

When used in scripts or pipelines, `binsql` renders a single result and exits.

Example:

```bash
binsql -q "select count(*) as n from languages"   sqlite ./cms.data.sqlite
```

Driver‑specific default list‑tables queries are used when `-q` is omitted but stdout is not a TTY.

Output is a box‑drawing table similar to the TUI’s grid.

This form is kept for backwards compatibility. For scripting, prefer
[Command mode](#command-mode), which adds writes, transactions, bind
parameters, schema inspection and JSON/CSV output.

---

## Command mode

Command mode is the non‑interactive, scriptable half of `binsql`. It is
designed to be driven by shell scripts, CI jobs and AI coding agents:
predictable flags, machine‑readable output, and guardrails around anything
destructive.

```bash
binsql <command> [flags] [arguments]
```

| Command | What it does |
| --- | --- |
| `query` | Run a read‑only query and print the result set |
| `exec` | Run statements that modify data or schema |
| `tables` | List tables and views |
| `describe` | Show the columns of one table |
| `schema` | Dump the schema of every table (or a subset) |
| `count` | Count rows in a table |
| `head` | Show the first rows of a table |
| `conn` | Manage saved connection profiles |
| `tui` | Launch the interactive terminal UI |
| `version`, `help` | Version, and per‑command help |

Run `binsql help <command>` for the full flag list of any command.

### Connecting

Every database‑touching command accepts the same connection flags:

```
-c, --conn NAME     use a saved profile
-d, --driver NAME   sqlite | postgres | mssql | mysql
-D, --dsn STRING    connection string or file path
```

The driver is **inferred from the DSN** when it is unambiguous, so
`--dsn ./app.db`, `--dsn postgres://...` and `--dsn "user:pw@tcp(host)/db"`
all work without `--driver`.

A DSN may also be an **Azure Key Vault reference** — see
[Azure Key Vault](#azure-key-vault):

```bash
binsql conn add prod --dsn "keyvault://my-vault/sql-connection-string"
```

Connections are resolved in this order:

1. `--dsn` (with `--driver`, or inferred)
2. `--conn NAME`
3. `BINSQL_DSN` / `BINSQL_CONN` from the environment
4. the profile marked as default

Environment variables: `BINSQL_CONN`, `BINSQL_DRIVER`, `BINSQL_DSN`,
`BINSQL_FORMAT`, `BINSQL_READONLY`, `BINSQL_CONFIG`, `BINSQL_SECRET_TTL`,
`BINSQL_AZURE_CREDENTIAL`, `BINSQL_KEYVAULT_SUFFIX`.

### Saved connections

Profiles live in `~/.config/binsql/connections.json` (override with
`BINSQL_CONFIG`), written with owner‑only permissions since a DSN usually
contains a password. Passwords are masked whenever a DSN is printed.

```bash
binsql conn add local --dsn ./app.db
binsql conn add prod  --dsn "postgres://u:pw@host/db" --readonly
binsql conn default local
binsql conn list
binsql conn test prod
```

Mark production databases with `--readonly`. `binsql` then refuses every
mutating statement on that profile, **before it even connects** — regardless
of which command is used.

### Azure Key Vault

A profile can hold a *reference* to a secret instead of a connection string,
so no credential is ever written to disk:

```bash
binsql conn add prod --dsn "keyvault://my-vault/sql-connection-string" --readonly
```

Accepted forms — the last is what the Azure portal's "Secret Identifier"
button copies:

```
keyvault://my-vault/secret-name
keyvault://my-vault/secret-name/version
keyvault://my-vault.vault.usgovcloudapi.net/secret-name
https://my-vault.vault.azure.net/secrets/secret-name
```

`conn add` reads the secret once to confirm you can reach it and to record
which driver it points at, then stores only the reference:

```json
{
  "prod": {
    "driver": "postgres",
    "dsn": "keyvault://my-vault/sql-connection-string",
    "readonly": true
  }
}
```

Pass `--no-verify` (together with `--driver`) to register a profile without
contacting Azure.

#### Authentication

Secrets are read with `DefaultAzureCredential`, which covers `az login` on a
developer machine, a managed identity on Azure, and
`AZURE_CLIENT_ID`/`AZURE_TENANT_ID`/`AZURE_CLIENT_SECRET` in CI. The
identity needs **Key Vault Secrets User** (or `get` on the secret).

That default chain probes for a managed identity first, which on a laptop
costs several seconds waiting for a timeout. If you authenticate with the
Azure CLI, skip it:

```bash
export BINSQL_AZURE_CREDENTIAL=cli   # default | cli | env | managed
```

On this machine that took a vault fetch from ~6s down to ~2s.

#### Caching

A resolved secret is cached under the config directory for
`--secret-ttl` (default 15 minutes), so a run of several commands pays the
vault round-trip once.

```bash
binsql conn cache            # how many entries, where, and the TTL
binsql conn cache --clear    # forget everything, including the local key
```

Entries are encrypted with AES-256-GCM under a key in the same directory,
and both files are written `0600`. Be clear about what that buys: because
the key sits next to the ciphertext, it protects against a secret being
picked up *incidentally* — by a backup, a directory sync, a shared screen,
or a `grep` across your home directory — not against someone who can already
read your files as you. Secret names are hashed, so the cache does not
disclose which vaults you use either.

To keep secrets off the disk entirely:

```bash
binsql query --conn prod "..." --secret-ttl 0
export BINSQL_SECRET_TTL=0        # or set it once
```

#### Azure SQL without a secret

For Azure SQL you often need no stored credential at all — authenticate to
the database directly with your Azure identity and skip Key Vault:

```bash
binsql conn add prod --driver mssql \
  --dsn "server=my.database.windows.net;database=app;fedauth=ActiveDirectoryAzCli"
```

Key Vault is the answer where a real secret exists: PostgreSQL, MySQL, or
SQL Server with SQL authentication.

### Output formats

```
-o, --format FORMAT   table (default), json, jsonl, csv, tsv,
                      vertical, markdown, raw, none
    --max-rows N      print at most N rows (0 = all)
    --max-width N     cap column width in table output
    --no-header       omit the header row
    --pretty          indent JSON
```

Data goes to **stdout**; status messages, warnings and errors go to
**stderr**, so a pipeline never has to strip chatter out of its input. With
`-o json` errors are emitted as JSON too, so a consumer sees one shape on
both paths.

`query` emits a stable envelope:

```json
{
  "columns": [{"name": "id", "type": "integer", "nullable": false}],
  "rows": [{"id": 1, "email": "a@example.com"}],
  "row_count": 1,
  "total_rows": 1,
  "truncated": false,
  "duration_ms": 0.8
}
```

Numbers stay JSON numbers and `NULL` becomes `null`. Non‑text binary values
are base64‑encoded with a `base64:` prefix. When `--max-rows` drops rows,
`truncated` is `true` and `total_rows` still reports the real count — output
is never silently shortened.

### Queries

```bash
binsql query "select id, email from users order by id limit 20"
binsql query "select * from orders where customer_id = ?" --arg int:31 -o json
binsql query -f report.sql -o csv > report.csv
echo "select count(*) from events" | binsql query -o raw
```

Bind values with `?` **regardless of driver** — placeholders are rewritten to
`$1` for PostgreSQL and `@p1` for SQL Server. Question marks inside string
literals and comments are left alone. Values are strings unless prefixed
with a type:

```
--arg int:42   --arg float:1.5   --arg bool:true   --arg null:   --arg str:007
```

`query` refuses statements that would modify the database and points you at
`exec` instead; `--allow-write` overrides it.

### Changes

```bash
binsql exec "insert into users (email) values (?)" --arg a@example.com
binsql exec "update users set active = 0 where id = ?" --arg int:9
binsql exec -f migration.sql --dry-run
binsql exec -f migration.sql
```

- Multiple statements separated by `;` run in order. Two or more are wrapped
  in **one transaction** by default, so a failure part‑way through rolls the
  whole batch back rather than leaving a half‑applied migration.
- `--dry-run` runs the batch in a transaction and rolls it back, reporting
  what it *would* have done. (MySQL commits DDL implicitly; `binsql` warns
  when a dry run cannot actually undo the batch.)
- `--tx` / `--no-tx` force transaction behaviour explicitly.
- `SELECT`s inside a script still print their results, so mixed scripts work.

Statement splitting understands string literals, quoted identifiers,
comments, MySQL backticks and backslash escapes, SQL Server `[brackets]` and
`GO` batches, and PostgreSQL `$$` dollar‑quoted bodies — a `;` inside any of
those does not split the script.

### Safety

`binsql` refuses, unless `--force` is given:

- `UPDATE` or `DELETE` with no `WHERE` clause
- `DROP` or `TRUNCATE`

and, on a `--readonly` profile or with `BINSQL_READONLY=1`, every mutating
statement. Those checks run **before connecting**, so a mistake never
reaches the database.

Exit codes: `0` success, `1` runtime error, `2` usage error.

### Inspecting schema

```bash
binsql tables
binsql tables --like user
binsql describe users
binsql schema -o json --pretty     # every table, one call
binsql count orders --where "status = 'open'"
binsql head orders -n 20
```

`describe` reports column name, type, nullability, primary key and default
for every supported driver. `head` and `count` build driver‑correct SQL, so
`head` uses `TOP` on SQL Server and `LIMIT` elsewhere.

### Driving binsql from an AI agent

Command mode was built with tools like Claude Code in mind. A useful setup:

```bash
# once, per database
binsql conn add myapp --dsn ./app.db
binsql conn add prod  --dsn "postgres://..." --readonly
```

Then an agent can work against `--conn myapp` without ever handling the
credentials — and with a `keyvault://` profile the credential is not on the
machine at all, only the reference:

```bash
binsql conn add prod --dsn "keyvault://my-vault/prod-conn" --readonly
```

Recipes worth knowing:

```bash
binsql schema --conn myapp -o json          # whole schema in one call
binsql query --conn myapp "<sql>" -o json --max-rows 50
binsql exec  --conn myapp -f change.sql --dry-run   # verify, then rerun without --dry-run
```

Why it behaves well unattended:

- **stdout is only data**, so JSON can be parsed directly.
- **Errors are JSON** under `-o json`, and exit codes distinguish usage
  mistakes (`2`) from runtime failures (`1`).
- **`--max-rows` bounds output**, and reports `truncated` rather than
  silently shortening a result.
- **Writes are opt‑in**: `query` refuses them, `exec` blocks unqualified
  `DELETE`/`UPDATE` and `DROP`/`TRUNCATE` without `--force`, and a
  `--readonly` profile refuses them outright before connecting.
- **`--dry-run` is a real transaction rollback**, so a migration can be
  checked before it is applied.

---

## Drivers and adapters

Each database has a small adapter implementing a common interface (`db.DB`):

- `internal/db/sqlite`
- `internal/db/postgres`
- `internal/db/mssql`
- `internal/db/mysql`

The app layer (`internal/app`) selects an adapter based on the chosen driver and DSN.  
The UI (`internal/ui`) is driver‑agnostic and only talks to that interface.

Adding a new database is mostly a matter of implementing that interface and updating the driver enum/factory.

---

## Notes and caveats

- MSSQL GUIDs (`uniqueidentifier`) are formatted as canonical GUID strings.
- Other MSSQL binary columns are rendered as hex (`0x...`) to avoid corrupting the table layout with non‑UTF‑8 bytes.
- Azure AD support for SQL Server currently targets Azure CLI (`fedauth=ActiveDirectoryAzCli`). Other `fedauth` modes may require additional environment configuration.

---

## License

MIT.
