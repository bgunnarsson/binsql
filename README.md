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
- Non‑interactive mode for one‑off queries (suitable for scripting)

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

General form:

```bash
binsql [flags] <sqlite|postgres|mssql|mysql> <database-path-or-dsn>
```

Only `-q` is supported as a flag; everything else is positional.

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
