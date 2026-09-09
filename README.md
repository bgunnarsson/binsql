# binsql

A database IDE for the terminal. Several databases open at once, a tree you can
walk, tabbed query consoles, and a result grid — DataGrip's shape, without
leaving the terminal.

Version 3 is a rewrite in Rust. The Go implementation is archived under
[`_old/`](_old/) and still builds; see [Status](#status) for what has and has
not been carried across.

```
╭ Databases 2/3 ───────────╮ artist   +
│▾ ● local  PostgreSQL     │╭ Query · local / app ───────────────────────────────────╮
│  ▾ app  ·                ││SELECT * FROM "artist" LIMIT 500                        │
│    ▾ public              ││                                                        │
│      ▾ Tables  12        │╰────────────────────────────────────────────────────────╯
│        artist            │╭ Results 1/3 · 3 rows in 0.24ms · id int4 ──────────────╮
│        album             ││      id name             founded                       │
│    ▸ Views  2            ││  1    1 Portishead          1991                       │
│  ▸ analytics             ││  2    2 Boards of Canada    1986                       │
│▾ ● prod  SQL Server  ro  ││  3    3 Autechre            NULL                       │
│▸ ○ warehouse  MySQL      │╰────────────────────────────────────────────────────────╯
╰──────────────────────────╯
 RESULTS  3 rows in 0.24ms      ⌃R run · ⌃K commands · ⌃T tab · F1 help · ⌃Q quit
```

## Install

```sh
cargo build --release
# the binary lands at target/release/binsql
```

Rust 1.90 or newer.

## Use

```sh
binsql                          # open the saved data sources
binsql local                    # open one saved data source by name
binsql ./app.db                 # open a DSN directly, driver inferred
binsql --driver mysql "user:pass@tcp(host:3306)/app"
```

Data sources live in `~/.config/binsql/connections.json`, honouring
`BINSQL_CONFIG` and `XDG_CONFIG_HOME`. **A v2 config opens unchanged** — the
file format is the same, and everything v3 adds is optional. Add a data source
from inside the app with `⌃N` rather than editing the file.

```json
{
  "default": "local",
  "connections": {
    "local": {
      "driver": "postgres",
      "dsn": "postgres://app@localhost:5432/app",
      "open_on_start": true
    },
    "prod": {
      "driver": "mssql",
      "dsn": "server=tcp:db.example.net,1433;database=app;fedauth=ActiveDirectoryDefault",
      "readonly": true
    }
  }
}
```

`readonly` refuses every mutating statement on that data source before anything
reaches the server — how a production database should be registered.
`open_on_start` connects it when binsql launches; with none set, the `default`
one is opened.

## Keys

| | |
| --- | --- |
| `⌃R` | Run the query — or just the selection, if there is one |
| `⌃K` | Command palette |
| `⌃T` / `⌃W` | New console / close console |
| `⌥1`…`⌥9` | Jump to a console |
| `⌃N` | New data source |
| `Tab` / `⇧Tab` | Cycle panes |
| `⌥h` `⌥k` `⌥j` | Databases / query / results |
| `F1` or `?` | Help |
| `F5` | Reload the selected node from the server |
| `⌃Q` | Quit |

In the tree: `j`/`k` to move, `l`/`Space` to expand, `h` to collapse or step up,
`Enter` to connect or open a table, `n`/`e`/`d` to add, edit or disconnect a
data source. In the grid: `hjkl` by cell, `g`/`G` and `0`/`$` for the edges,
`Enter` for the full value of a cell the grid had to truncate.

## Databases

| Driver names | Connection string |
| --- | --- |
| `sqlite`, `sqlite3` | a path, or `sqlite://path` |
| `postgres`, `postgresql`, `pg` | `postgres://user:pass@host:5432/db`, or the libpq `host=… dbname=…` form |
| `mssql`, `sqlserver`, `azuresql` | ADO `server=tcp:host,1433;…`, or `sqlserver://user:pass@host:1433?database=db` |
| `mysql`, `mariadb` | `mysql://user:pass@host:3306/db`, or `user:pass@tcp(host:3306)/db` |

The driver is inferred from the connection string unless `--driver` says
otherwise. Both DSN spellings v2 accepted are still accepted; the go-flavoured
ones are translated internally.

### Azure AD for SQL Server

A connection string containing `fedauth=` authenticates with an Azure AD token
instead of a password, exactly as it did in v2. The token comes from the Azure
CLI, so `az` must be on `PATH` and `az login` must have been run.

### Azure Key Vault references

A data source can name a secret instead of holding one, so the config on disk
need contain no credential at all:

```json
{
  "connections": {
    "prod": {
      "driver": "mssql",
      "dsn": "keyvault://kv-eimskip-prd/ConnectionStrings--umbracoDbDSN",
      "readonly": true
    }
  }
}
```

Accepted forms — `keyvault://` and `azkv://` are the same thing:

```
keyvault://my-vault/secret-name
keyvault://my-vault/secret-name/version
keyvault://my-vault.vault.azure.net/secret-name
https://my-vault.vault.azure.net/secrets/secret-name[/version]
```

The reference is resolved just before connecting, and the result is cached for
15 minutes so restarting binsql does not mean waiting on the vault again.
Cached secrets are encrypted with AES-256-GCM under a key kept beside them, and
both files are `0600`. Be clear about what that buys: because the key sits next
to the ciphertext it guards against a secret being picked up incidentally — by
a backup, a directory sync, a shared screen, a grep across your home directory
— not against someone who can already read your files as you. The cache format
is v2's, so both versions share it while both are installed.

| Variable | Effect |
| --- | --- |
| `BINSQL_SECRET_TTL` | Cache lifetime in seconds. `0` resolves every time and writes nothing to disk. |
| `BINSQL_KEYVAULT_SUFFIX` | Key Vault DNS suffix, for sovereign clouds. |

Secrets are fetched through the Azure CLI, so this needs `az` and `az login`
just as `fedauth=` does. **This is narrower than v2**, which linked the Azure
SDK and could also use a managed identity or an `AZURE_CLIENT_ID` service
principal via `BINSQL_AZURE_CREDENTIAL`. Those arms served CI, which is command
mode's territory; they come back with it.

## Design

Two crates:

- **`binsql-core`** — connections, introspection and query execution, with every
  database difference resolved behind an `Adapter`. Nothing above it knows which
  engine is on the other end. SQLite, PostgreSQL and MySQL go through `sqlx`;
  SQL Server goes through `tiberius`.
- **`binsql`** — the terminal front end: `app` holds state and drives background
  work over a channel, `ui` draws it. The core is a library so a headless
  command mode can be added over the same guarantees.

A `Session` is a data source, not a database. Backends that cannot read across
their own databases on one connection — Postgres — grow a second connection
lazily when you expand a sibling catalog; the ones that can, do not.

## Tests

```sh
cargo test --workspace
```

The suite includes an end-to-end test that connects to a real SQLite database,
walks the tree, runs a query and renders the whole layout to a test backend, so
the thing people look at is asserted rather than assumed.

## Status

This rewrite is TUI-first. What works today is everything above. What has **not**
been carried across from v2 yet:

- **Command mode** (`binsql query`, `exec`, `inspect`, with JSON/CSV/markdown
  output and `--dry-run`). The core is built as a library to take it, but the
  verbs are not implemented — scripts and agents on v2 should keep using the
  archived binary until they are.
- **Managed identity and service-principal credentials** for Key Vault. The
  references themselves work; only the CLI credential is wired up, for the
  reason given under [Azure Key Vault references](#azure-key-vault-references).
- Exporting a result set, editing values in the grid, query history, cancelling
  a running query, and filtering the tree.

## Licence

MIT.
