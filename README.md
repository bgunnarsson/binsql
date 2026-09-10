# binsql

A database IDE for the terminal. Several databases open at once, a tree you can
walk, tabbed query consoles, and a result grid — DataGrip's shape, without
leaving the terminal. And the same databases without the UI, for a script or an
agent: see [Command mode](#command-mode).

Version 3 is a rewrite in Rust. The Go implementation is archived under
[`_old/`](_old/) and still builds; see [Status](#status) for what has and has
not been carried across.

```
 ✻  acme/local › app                        PostgreSQL 16.2  ·  db.acme.internal:5432 
╭─ Databases ───────── 2/3 ─╮ artist   +
│ ▾  acme  2                │╭─ Query ───────────────────────────────────────── 1/2 ─╮
│   ▾  local  PostgreSQL    ││SELECT * FROM "artist" LIMIT 500                       │
│     ▾  app  ·             │╰───────────────────────────────────────────────────────╯
│       ▾  public           │╭─ Results · 3 rows in 0.24ms · id int4 ────────── 1/3 ─╮
│         ▾ Tables  12      ││      id name             founded                      │
│▌          artist          ││  1    1 Portishead          1991                      │
│            album          ││  2    2 Boards of Canada    1986                      │
│         ▸ Views  2        ││  3    3 Autechre            NULL                      │
│   ▸  prod  SQL Server  ro │╰───────────────────────────────────────────────────────╯
│ ▸  warehouse  MySQL       │
╰───────────────────────────╯
 3 rows in 0.24ms                  ⇥ panes · ⌃R run · ⌃K commands · F1 help · ⌃Q quit 
```

The header answers one question — where `⌃R` sends this SQL. The left is what
you call that connection and which database it is in; the right is what it
actually reaches, so two connections both nicknamed `prod` are told apart. A
read-only data source says so there too.

`Enter` on a row opens it as a record, so a value too long for the grid is
still readable:

```
╭─ Row 17 of 500 · comment nvarchar ────────────── 4/6 fields ─╮
│id        17                                                  │
│delta     1                                                   │
│entity    User                                                │
│comment   "binni@vettvangur.is" <binni@vettvangur.is> changed  │
│          the publication schedule for the shipping page      │
│reviewed  NULL                                                │
│↑↓ field · ↵ value · ←→ record · Esc close                    │
╰──────────────────────────────────────────────────────────────╯
```

binsql opens on a splash with the version, how many data sources are
registered, and the three keys worth knowing. Any key dismisses it.

## Install

**v3 is not tagged yet.** The crate says `3.0.0-dev` and
[the releases page](https://github.com/bgunnarsson/binsql/releases) still ends
at v2, which is the Go build. Until there is a tag, build it:

```sh
cargo build --release
# the binary lands at target/release/binsql
```

Tagging is what makes a release: `v3.0.0` sends GitHub Actions off to build
darwin-arm64, darwin-amd64, linux-amd64, linux-arm64 and windows-amd64, and
attaches the five archives and a `checksums.txt` to the tag. Each unpacks into a
directory of its own, so it cannot overwrite anything where it lands:

```sh
tar -xzf binsql-3.0.0-darwin-arm64.tar.gz
sudo mv binsql-3.0.0-darwin-arm64/binsql /usr/local/bin/
```

Rust 1.90 or newer to build, and a Nerd Font in your terminal either way —
binsql draws the tree and the pane chrome with the same glyphs binvim does, and
sits beside it in the same font. Without one the icons render as boxes; nothing
else is affected.

## Use

```sh
binsql                          # open the saved data sources
binsql eimskip/prod             # open one saved data source
binsql scratch                  # a bare name, when only one folder has it
binsql ./app.db                 # open a DSN directly, driver inferred
binsql --driver mysql "user:pass@tcp(host:3306)/app"
```

A first argument that names a verb — `query`, `exec`, `inspect` — is
[command mode](#command-mode) instead; anything else is a data source to open.
Anything binsql can neither find among the saved ones nor read as a connection
string is an error naming both ways out, since a mistyped verb lands there as
readily as a bad DSN. `-h`, `-V` and `--debug-keys` print and stop.

Data sources live in `~/.config/binsql/connections.json`, honouring
`BINSQL_CONFIG` and `XDG_CONFIG_HOME`. Add one from inside the app with `⌃N`
rather than editing the file.

A connection can sit at the top level, or inside a **folder** — a client, a
project — which becomes a group in the sidebar:

```json
{
  "default": "eimskip/prod",
  "connections": {
    "eimskip": {
      "prod": {
        "driver": "mssql",
        "dsn": "keyvault://kv-eimskip-prd/ConnectionStrings--umbracoDbDSN",
        "readonly": true
      },
      "local": {
        "driver": "mssql",
        "dsn": "keyvault://kv-eimskip-local/ConnectionStrings--umbracoDbDSN",
        "description": "the docker one"
      }
    },
    "osar": {
      "prod": {
        "driver": "mssql",
        "dsn": "keyvault://kv-osar-prd/ConnectionStrings--umbracoDbDSN",
        "readonly": true
      }
    },
    "scratch": {
      "driver": "sqlite",
      "dsn": "/tmp/scratch.db",
      "open_on_start": true
    }
  }
}
```

```
╭─ Databases ───────── 1/4 ─╮
│ ▾  eimskip  2             │
│   ▾  local  SQL Server    │
│   ▸  prod  SQL Server  ro │
│ ▸  osar  1                │
│ ▸  scratch  SQLite        │
╰───────────────────────────╯
```

Folders start closed, so a machine with a dozen clients on it opens to a list
of clients rather than to every database at once. One that connects on its own
— the `default`, or anything flagged `open_on_start` — opens its folder, so it
is never working away out of sight.

Folders are one level deep, and a connection is named `folder/name` everywhere
it is referred to — `binsql eimskip/prod`, the `default` key, the command
palette. A bare name still works when only one folder has it, so `binsql
scratch` and `binsql local` are fine above but `binsql prod` is not: it names
two things, so it names neither. **A v2 config opens unchanged** — connections
written at the top level stay there, and everything v3 adds is optional.

`readonly` refuses every mutating statement on that data source before anything
reaches the server — how a production database should be registered. The whole
script is checked, not its first word: a `DELETE` under a comment, behind a
`SELECT 1;`, or fronted by a `WITH` is still a write, and a statement binsql
cannot classify counts as one too.
`open_on_start` connects it when binsql launches; with none set, the `default`
one is opened. `description` is a note to yourself and is optional, as are all
three — a connection is a `driver` and a `dsn` and nothing else has to be there.

The file is written back the way it was found: folders stay folders, top-level
connections stay at the top level, and the three optional keys are omitted
rather than written out as `false`. It is saved through a temp file so a failed
write cannot truncate a config holding credentials, and both it and its
directory are owner-only.

## Keys

| | |
| --- | --- |
| `⌃R` | Run the query — or just the selection, if there is one |
| `⌃C` | Cancel the running query |
| `⌃K` | Command palette |
| `⌃T` / `⌃W` | New console / close console |
| `⌥1`…`⌥9` | Jump to a console |
| `⌃PgUp` / `⌃PgDn` | Previous / next console |
| `⌃N` | New data source |
| `Tab` / `⇧Tab` | Cycle panes |
| `⌥h` `⌥j` `⌥k` `⌥l` | The pane that way — databases, results, query, query |
| `F1` or `?` | Help |
| `F5` | Reload the selected node from the server |
| `⌃Q` | Quit |

`⌃Q` is read before anything else can claim it. Raw mode has already taken `⌃C`
away from the terminal — it arrives as an ordinary key and cancels the query,
not the program — so a modal that swallowed `⌃Q` would leave no way out at all.

In the tree: `j`/`k` to move, `⌃D`/`⌃U` by half a page, `g`/`G` for the ends,
`l`/`Space` to expand, `h` to collapse or step up, `Enter` to connect or open a
table, `r` to reload it from the server, and `n`/`e`/`d` to add, edit or
disconnect a data source. In the grid: `hjkl` by cell, `⌃D`/`⌃U` by half a page,
`g`/`G` and `0`/`$` for the edges, `Enter` opens the whole record with every
column stacked and long values wrapped, where `↑`/`↓` move between fields,
`Enter` again opens the one you are on in full — JSON re-indented — and `←`/`→`
step between records without closing it. Moving between fields moves the grid's
cursor with it, so paging through records in the modal does not lose your place.

In the query editor: `⌃A` selects all — typing over a selection replaces it —
`⌃U` or `⌃⌫` deletes back to the start of the line, `⌃X` cuts, `⌃Z` undoes, and
`Esc` hands focus back to the tree.

**Undo works in runs, not letters.** One `⌃Z` takes back a word, not the last
character of it; a space, a line break, or anything done in one go — a paste, a
cut, a selection typed over — is where a run ends. Redo puts back exactly what
undo took.

Redo is `⌃⇧Z`, or `⌃Y` where that is not available: `⌃⇧Z` and `⌃Z` are the same
byte to a terminal speaking the old encoding, so telling them apart needs the
kitty keyboard protocol. Ghostty, Kitty, WezTerm and foot have it, iTerm2 has
it behind a setting, and Terminal.app has no such mode. binsql asks for it at
startup and goes without when the answer is no.

`binsql --debug-keys` prints what your terminal actually sends, which is the
only way to settle a question like that.

## Mouse

A click focuses the pane it landed in and lands the cursor with it: a position
in the query, a cell in the grid, a row in the tree. Clicking a tab switches to
that console, and the `+` at the end of the strip opens one. Double-clicking
does whatever `Enter` would — connects a data source, opens a table, opens a
record. Dragging across the query selects, and `⌫` removes what was selected.
The wheel scrolls whatever the pointer is over, the query editor included.

**Both pane seams drag.** The sidebar's edge widens it, for a schema whose
table names are longer than the default fits — it never narrows below that
default, which is already what ordinary names want. The seam under the query
pane moves either way, so a query being written can have the room, or the
result it returns can. Neither pane can be squeezed out of existence.

All of this costs the terminal's own click-to-select, which is what mouse
reporting takes away; most terminals hand it back while `⇧` is held.

## Command mode

The same core without the terminal UI, for a script, a CI job or an agent:

```sh
binsql query "SELECT * FROM artist ORDER BY id"      # read
binsql exec  -f migration.sql --dry-run              # write, and keep nothing
binsql inspect                                       # what tables are there
binsql inspect artist                                # what columns has it got
```

The verbs are few and the split between them is a safety boundary rather than a
convenience. **`query` cannot write** — a mutating statement is refused before
it is sent, so a script that only reads can say so in the command it runs, and
whoever reads that script later can see it without reading the SQL. **`exec` is
the verb that writes**, and it is transactional: two or more statements are one
unit of work, so a failure part-way through leaves nothing behind.

Every command takes the same connection flags. With none of them, the `default`
data source is opened.

| | |
| --- | --- |
| `-c, --conn NAME` | a saved data source, `folder/name` or a bare name |
| `-D, --dsn STRING` | a connection string, used instead of a saved one |
| `-d, --driver NAME` | `sqlite` \| `postgres` \| `mssql` \| `mysql` (default: inferred) |

`BINSQL_CONN`, `BINSQL_DSN` and `BINSQL_DRIVER` say the same things through the
environment, and a flag beats the variable.

And the same output flags. `--format` decides the whole of what lands on
stdout; nothing else is ever written there, so the structured formats can be
piped straight into something that parses them.

| | |
| --- | --- |
| `-o, --format NAME` | `table` (default), `json`, `jsonl`, `csv`, `tsv`, `vertical`, `markdown`, `raw`, `none` |
| `--pretty` | indent JSON |
| `--no-header` / `--no-footer` | leave out the header row, or the trailing count |

```sh
binsql query "SELECT * FROM users LIMIT 20" -o json --pretty
binsql query -f report.sql --conn eimskip/prod -o csv > report.csv
binsql exec "UPDATE users SET active = 0 WHERE id = 9"
binsql inspect --conn scratch -o markdown
```

`query` takes `-f, --file FILE` (`-` for stdin, as does a bare pipe), `--limit N`
to stop after N rows (`0` for all of them), and `--allow-write` for the rare
statement that has to write from the reading verb. It runs exactly one
statement: handed a script, it says so and points at `exec` rather than running
the first and dropping the rest.

`exec` takes `-f, --file FILE`, and:

| | |
| --- | --- |
| `--dry-run` | run the batch in a transaction and roll it back — it really executes, and nothing is kept |
| `--tx` / `--no-tx` | force one transaction around the batch, or none |
| `--force` | permit `UPDATE`/`DELETE` with no `WHERE`, and `DROP`/`TRUNCATE` |

Without `--force`, the two statements most likely to be a mistake are refused
before they are sent. `--dry-run` is transactional everywhere except MySQL DDL,
which MySQL commits as it runs; binsql says so rather than letting a dry run
imply otherwise. A batch of two or more is one transaction by default; a lone
statement is not worth wrapping, and `--tx` is how to say it should be anyway.

`inspect` reads the connected database, and every schema in it:

| | |
| --- | --- |
| `--catalog NAME` | another database on the same connection |
| `--schema NAME` | one schema rather than all of them |

A table may carry its own schema — `binsql inspect dbo.orders` — which wins over
`--schema`, being the more specific of the two. Either way the name is matched
against what the server says it has rather than pasted into a query, so a
misspelling comes back as binsql saying there is no such table instead of as a
syntax error from the database.

Exit codes are `0` for success, `1` for a database that said no, and `2` for a
usage mistake — so a script can tell "you asked wrong" from "it did not work".
⌃C cancels the running query the same way it does in the TUI.

## Databases

| Driver names | Connection string |
| --- | --- |
| `sqlite`, `sqlite3` | a path, or `sqlite://path` |
| `postgres`, `postgresql`, `pg`, `pgx` | `postgres://user:pass@host:5432/db`, or the libpq `host=… dbname=…` form |
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
principal via `BINSQL_AZURE_CREDENTIAL`. Shelling out to `az` was chosen so
binsql has one Azure story rather than two — it is the same mechanism
`fedauth=` uses — and it was enough while there was nothing here for CI to run.
Command mode has since arrived and CI is exactly what it is for, so that is now
a gap rather than a deferral: see [Status](#status).

## Design

Two crates:

- **`binsql-core`** — connections, introspection and query execution, with every
  database difference resolved behind an `Adapter`. Nothing above it knows which
  engine is on the other end. SQLite, PostgreSQL and MySQL go through `sqlx`;
  SQL Server goes through `tiberius`.
- **`binsql`** — the terminal front end and the command line: `app` holds state
  and drives background work over a channel, `ui` draws it, `cli` is the same
  core with neither. Keeping the core a library is what let command mode be
  built over the same guarantees rather than beside them — a read-only data
  source refuses a write in one line of `Session`, and both front ends inherit
  it without restating it.

### Look

Two borrowings, each for what it is good at.

**Panes and overlays follow binvim**, which shares the terminal:

- **Two surfaces.** The body — query buffer, result grid — is `#1e1e2e`;
  chrome — the tree, the tab strip, the header and status lines, every overlay
  — is `#181825`, so chrome reads as layered above rather than painted in.
- **binvim's chrome roles**, same names and values: `foreground, dim, emphasis,
  surface, border, accent, accent_secondary, error, warning, hint`. `theme.rs`
  is the only file that names a colour; everything else asks for a role.
- **binvim's popup form.** Title after a single dash in the top border, a
  counter at the right end of the same border, and `▌` in `emphasis` down the
  left of the selected row.
- **A scrim behind open modals.** Everything else blends toward the background
  while a modal is up, so the modal is plainly the thing being talked to and
  the layout stays as context rather than as competition.
- **Nerd Font glyphs** for servers, databases, schemas, tables, views, columns
  and keys.

**The header and status line follow Claude Code**, which is quieter: one mark in
its coral `#d97757`, then plain text separated by `·`. These started as
binvim's powerline segments with a chip naming the focused pane, which was a
mistranslation — binvim's chips announce a *mode*, binsql has none, and the
focused pane already says so with its border.

### Cancelling

`⌃C` calls off the running query. The console is yours again immediately; what
it costs the server depends on what the server offers. Postgres and MySQL are
told to stop — a second connection sends `pg_cancel_backend` or `KILL QUERY`,
which is the only way either of them hears it, since neither notices a client
that has stopped listening. SQL Server has no such statement and tiberius does
not expose TDS's attention signal, so the connection is dropped and replaced,
which the server reads as a disconnect and abandons the batch for. SQLite runs
in this process and has no server to call off.

A `Session` is a data source, not a database. Backends that cannot read across
their own databases on one connection — Postgres — grow a second connection
lazily when you expand a sibling catalog; the ones that can, do not.

## Tests

```sh
cargo test --workspace
```

The suite includes an end-to-end test that connects to a real SQLite database,
walks the tree, runs a query and renders the whole layout to a test backend, so
the thing people look at is asserted rather than assumed. Command mode is
tested by running the built binary as a subprocess and reading its stdout, its
stderr and its exit code, because those three are its whole interface.

`cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
warnings` and this suite run on every push and pull request, and again on a tag
before anything is built — so a release is never the first time the gate has
seen a commit. The release workflow also runs by hand, which builds all five
targets and publishes nothing, and refuses a tag that disagrees with
`Cargo.toml`: a release that reports the wrong version reports it for the rest
of its life.

## Status

What works today is everything above. What has **not** been carried across from
v2 yet:

- **Bind arguments.** v2's `--arg` passed values as `?` placeholders, so a
  script never built SQL by concatenation. Adding them means teaching the
  adapters to bind parameters, which is a change to every one of them; until
  then, command mode takes SQL and no values.
- **Managing data sources from the command line.** v2 had `binsql conn add`.
  In v3 a data source is added with ⌃N in the TUI, or by editing
  `connections.json`.
- **Managed identity and service-principal credentials** for Key Vault. The
  references themselves work, and so does the CLI credential; the other two arms
  of v2's chain are missing, which now matters because command mode is here and
  a CI job has no `az login` to lean on.
- Exporting a result set, editing values in the grid, query history, and
  filtering the tree.

## Licence

MIT.
