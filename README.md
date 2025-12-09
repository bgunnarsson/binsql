#https://github.com/bgunnarsson/binsql binsql

`binsql` is a small terminal UI for exploring SQL databases. It supports SQLite, PostgreSQL, SQL Server (including Azure AD auth via Azure CLI), and MySQL from a single binary.

The goal is a fast, keyboard‑driven way to inspect schemas and data without leaving the terminal.

---

## Features

- Interactive REPL with line editing
- Box‑drawing table output
- Row expansion (`\e [rowNumber]`)
- Driver‑agnostic core with per‑database adapters
- Support for:
  - **SQLite**
  - **PostgreSQL**
  - **SQL Server** (including Azure AD via `fedauth=ActiveDirectoryAzCli`)
  - **MySQL**
- Non‑interactive mode for one‑off queries (suitable for scripting)

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

This produces platform‑specific binaries in `./dist` (names like `binsql-darwin-arm64`, `binsql-linux-amd64`, etc.).

---

## Usage

General form:

```bash
binsql [flags] <sqlite|postgres|mssql|mysql> <database-path-or-dsn>
```

Only `-q` is supported as a flag; everything else is positional.

- First argument: **driver**
- Second argument: **database path or DSN** (driver‑specific)
- If `-q` is omitted and stdout is a TTY → interactive TUI
- If `-q` is provided or stdout is not a TTY → non‑interactive, prints a single result table and exits

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

## Interactive commands

Once connected (any driver), you get a prompt:

```text
BINSQL [sqlite]   \dt tables  ·  \e [n] expand  ·  \q quit
>>> 
```

Supported commands:

- `\q` – quit
- `\dt` – list tables (driver‑aware)
- `\e [rowNumber]` – expand a row from the last result set

Any other input is treated as SQL and sent to the connected database.

### Examples

List tables:

```text
>>> \dt
List of relations
┌─────────────────────────────┐
│ Table                       │
├─────────────────────────────┤
│ public.languages            │
│ public.documents            │
│ public.media                │
└─────────────────────────────┘
(3 rows)
```

Select and expand a row:

```text
>>> select * from languages
Use \e [rowNumber] to expand a row (example: \e 1).
┌────────┬────────┬──────────────┬─────────────┐
│ id     │ code   │ name         │ isdefault   │
├────────┼────────┼──────────────┼─────────────┤
│ ...    │ en-US  │ English (US) │ true        │
└────────┴────────┴──────────────┴─────────────┘
(1 rows)

>>> \e 1
Row 1
id        > ...
code      > en-US
name      > English (US)
isdefault > true
```

---

## Non‑interactive mode

When used in scripts or pipelines, `binsql` renders a single result and exits.

Example:

```bash
binsql -q "select count(*) as n from languages" sqlite ./cms.data.sqlite
```

Driver‑specific default list‑tables queries are used when `-q` is omitted but stdout is not a TTY.

---

## Drivers and adapters

Each database has a small adapter implementing a common interface (`db.DB`):

- `internal/db/sqlite`
- `internal/db/postgres`
- `internal/db/mssql`
- `internal/db/mysql`

The app layer (`internal/app`) selects an adapter based on the chosen driver and DSN. The UI (`internal/ui`) is driver‑agnostic and only talks to the interface.

Adding a new database is mostly a matter of implementing that interface and updating the driver enum/factory.

---

## Notes and caveats

- MSSQL GUIDs (`uniqueidentifier`) are formatted as canonical GUID strings.
- Other MSSQL binary columns are rendered as hex (`0x...`) to avoid corrupting the table layout with non‑UTF‑8 bytes.
- Azure AD support for SQL Server currently targets Azure CLI (`fedauth=ActiveDirectoryAzCli`). Other `fedauth` modes may require additional environment configuration.

---

## License

MIT.

