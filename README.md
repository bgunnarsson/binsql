# binsql

A database IDE for the terminal. Several databases open at once, a tree you can
walk, tabbed query consoles, and a result grid — DataGrip's shape, without
leaving the terminal. And the same databases without the UI, for a script or an
agent: see [Command mode](#command-mode).

Version 3 is a rewrite in Rust; [Status](#status) says what has and has not been
carried across from the Go version.

The header answers one question — where `⌃R` will send this SQL. What you call
the connection and which database it is in on the left, what it actually reaches
on the right, so two connections both nicknamed `prod` are told apart. `Enter`
on a result row opens it as a record with every column stacked and long values
wrapped, so a value too long for the grid is still readable. binsql opens on a
splash with the version, how many data sources are registered, and the three
keys worth knowing; any key dismisses it.

## Install

Download the archive for your platform from [the releases
page](https://github.com/bgunnarsson/binsql/releases) — darwin-arm64,
darwin-amd64, linux-amd64, linux-arm64 or windows-amd64 — and check it against
the `checksums.txt` beside it. Each unpacks into a directory of its own, so it
cannot overwrite anything where it lands; put the `binsql` inside it on your
`PATH`. Or build it:

```sh
cargo build --release
# the binary lands at target/release/binsql
```

Tagging is what makes a release: a `v*` tag that agrees with `Cargo.toml` sends
GitHub Actions off to build the five archives and attach them to it.

Rust 1.90 or newer to build, and a Nerd Font in your terminal either way —
binsql draws the tree and the pane chrome with the same glyphs binvim does.
Without one the icons render as boxes; nothing else is affected.

## Use

```sh
binsql                          # open the saved data sources
binsql eimskip/prod             # open one saved data source
binsql scratch                  # a bare name, when only one folder has it
binsql ./app.db                 # open a DSN directly, driver inferred
binsql --driver mysql "user:pass@tcp(host:3306)/app"
```

A first argument that names a verb — `query`, `exec`, `inspect` — is
[command mode](#command-mode); anything else is a data source to open. Anything
binsql can neither find among the saved ones nor read as a connection string is
an error naming both ways out, since a mistyped verb lands there as readily as a
bad DSN. `-h`, `-V` and `--debug-keys` print and stop.

Data sources live in `~/.config/binsql/connections.json`, honouring
`BINSQL_CONFIG` and `XDG_CONFIG_HOME`. Add one from inside the app with `⌃N`
rather than editing the file; it is written owner-only through a temp file, so a
failed write cannot truncate a config that holds credentials. A repository can
carry its own alongside it — see [project data sources](#project-data-sources).

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
    "scratch": {
      "driver": "sqlite",
      "dsn": "/tmp/scratch.db",
      "open_on_start": true
    }
  }
}
```

Folders start closed, so a machine with a dozen clients on it opens to a list of
clients rather than to every database at once. One that connects on its own —
the `default`, or anything flagged `open_on_start` — opens its folder rather
than working away out of sight.

Folders are one level deep, and a connection is named `folder/name` everywhere
it is referred to: `binsql eimskip/prod`, the `default` key, the command
palette. A bare name works when only one folder has it, so `binsql scratch` and
`binsql local` are fine above but `binsql prod` would not be if a second folder
had one — it would name two things, so it would name neither. A `default` that
has stopped naming one thing says so at startup rather than quietly opening
nothing. **A v2 config opens unchanged**; everything v3 adds is optional.

A `dsn` that reads `keychain://eimskip/prod` keeps the connection string in the
operating system's credential store rather than in the file — see [the
keychain](#the-keychain). `⌃N` does that by default, which is why the config
above is worth reading and worth committing.

Only `driver` and `dsn` are required. `readonly` refuses every mutating
statement before anything reaches the server — how a production database should
be registered — and checks the whole script rather than its first word: a
`DELETE` under a comment, behind a `SELECT 1;`, or fronted by a `WITH` is still
a write, and a statement binsql cannot classify counts as one too.
`open_on_start` connects it at launch. `description` is a note to yourself.

### Project data sources

A repository can carry its own. binsql looks for the nearest `.binsql.json` at
or above the working directory and stacks it over your config, so
`cd eimskip && binsql` shows that project's databases without them living in
your personal file. Command mode reads the same file.

```json
{
  "connections": {
    "eimskip": {
      "local": { "driver": "mssql", "dsn": "keychain://eimskip/local" }
    }
  }
}
```

The layers merge per connection rather than per file, so a project's
`eimskip/local` joins your `eimskip/prod` in **one** `eimskip` folder in the
sidebar rather than a second folder of the same name appearing beside it. Where
both define the same name, the project wins — it is the more specific of the
two. A DSN given on the command line sits above both and is never written
anywhere.

Since a connection string is a [keychain](#the-keychain) or [Key
Vault](#azure-key-vault-references) reference, this file holds names, drivers
and flags and nothing sensitive, which is what makes it worth committing. It is
written without touching its own permissions or the repository's; your config
keeps its owner-only treatment.

When a project file is in play, the `⌃N` form grows a **Saved in** field naming
the two files, and a new connection starts in the project you are standing in.
An existing one goes back where it came from. Switching that field moves the
connection between the files rather than copying it. `BINSQL_PROJECT` names one
explicitly, and set empty turns the search off.

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

In the tree: `j`/`k` to move, `⌃D`/`⌃U` by half a page, `g`/`G` for the ends,
`l`/`Space` to expand, `h` to collapse or step up, `Enter` to connect or open a
table, `r` to reload it from the server, and `n`/`e`/`d` to add, edit or
disconnect a data source.

In the grid: `hjkl` by cell, `⌃D`/`⌃U` by half a page, `g`/`G` and `0`/`$` for
the edges. `Enter` opens the record, where `↑`/`↓` move between fields, `Enter`
again opens the one you are on in full — JSON re-indented — and `←`/`→` step
between records without closing it. Moving between fields moves the grid's
cursor with it, so paging through records does not lose your place.

In the query editor: `⌃A` selects all — typing over a selection replaces it —
`⌃U` or `⌃⌫` deletes back to the start of the line, `⌃X` cuts, `⌃Z` undoes, and
`Esc` hands focus back to the tree.

**Undo works in runs, not letters.** One `⌃Z` takes back a word, not the last
character of it; a space, a line break, or anything done in one go — a paste, a
cut, a selection typed over — is where a run ends. Redo puts back exactly what
undo took.

Redo is `⌃⇧Z`, or `⌃Y` where that is not available: the two are the same byte to
a terminal speaking the old encoding, so telling them apart needs the kitty
keyboard protocol. Ghostty, Kitty, WezTerm and foot have it, iTerm2 behind a
setting, Terminal.app not at all. binsql asks at startup and goes without when
the answer is no. `binsql --debug-keys` prints what your terminal actually sends.

## Mouse

A click focuses the pane it landed in and lands the cursor with it: a position
in the query, a cell in the grid, a row in the tree. Clicking a tab switches to
that console, and the `+` at the end of the strip opens one. Double-clicking
does whatever `Enter` would. Dragging across the query selects, and `⌫` removes
what was selected. The wheel scrolls whatever the pointer is over, the query
editor included.

**Both pane seams drag.** The sidebar's edge widens it for a schema whose table
names are longer than the default fits, and never narrows below that default.
The seam under the query pane moves either way, so a query being written can
have the room, or the result it returns can. Neither pane can be squeezed out of
existence.

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

The split between the verbs is a safety boundary rather than a convenience.
**`query` cannot write** — a mutating statement is refused before it is sent, so
a script that only reads says so in the command it runs, where whoever reads
that script later can see it without reading the SQL. **`exec` is the verb that
writes**, and it is transactional: two or more statements are one unit of work,
so a failure part-way through leaves nothing behind.

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
stdout — nothing else is ever written there, so the structured formats pipe
straight into something that parses them.

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

A batch of two or more is one transaction by default; a lone statement is not
worth wrapping, and `--tx` is how to say it should be anyway. Without `--force`,
the two statements most likely to be a mistake are refused before they are sent.
`--dry-run` is transactional everywhere except MySQL DDL, which MySQL commits as
it runs — binsql says so rather than letting a dry run imply otherwise.

Both take `--arg VALUE` to fill a `?` placeholder, once per placeholder and in
order, so a script hands over values rather than building SQL out of them:

```sh
binsql query "SELECT * FROM orders WHERE customer_id = ?" --arg int:31 -o json
binsql exec  "UPDATE users SET active = ? WHERE last_seen < ?" --arg bool:false --arg 2024-01-01
```

A value is text unless a prefix says otherwise — `int:42`, `float:1.5`,
`bool:true`, `null:`, `json:{"a":1}`, or `str:` for text that happens to begin
with one of those. It travels beside the statement rather than inside it, so a
quote in a value is only ever a quote.

`?` is the spelling on every database. binsql rewrites it to `$1` for
PostgreSQL and `@P1` for SQL Server, and leaves a `?` inside a string or a
comment alone. Across a script the values are handed out left to right, and a
count that does not come out even is refused before any of it runs. PostgreSQL
is sent each value as the type its placeholder takes, so text that reads as a
date reaches a `date` column as a date; the others convert a value to the
column's type themselves. In a statement that binds values every other `?` is
a placeholder too, so there Postgres's jsonb `?` operators need their function
forms, such as `jsonb_exists`.

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
usage mistake, so a script can tell "you asked wrong" from "it did not work".
`⌃C` cancels the running query the same way it does in the TUI.

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

### The keychain

A connection string that belongs to one machine goes in that machine's
credential store — the macOS Keychain, the Windows Credential Manager, or the
Secret Service on Linux — and the config keeps only its name:

```json
{
  "connections": {
    "eimskip": {
      "local": {
        "driver": "mssql",
        "dsn": "keychain://eimskip/local"
      }
    }
  }
}
```

The **Stored in** field of the `⌃N` form chooses this, and it is the default;
`←`/`→` or space switches it back to keeping the string in the config. The entry
is filed under the service `binsql` and the connection's qualified name, so it
is recognisable in Keychain Access, and renaming a data source moves it. It is
read fresh on every connect rather than going through the secret cache below —
one secure store is the point.

This is what makes a `connections.json` worth sharing: with every string either
a keychain entry or a Key Vault reference, the file is a list of names, drivers
and flags with nothing sensitive in it.

Moving a string back out of the store is deliberate rather than a toggle: clear
the connection string in the form and type it again.

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

The reference is resolved just before connecting and cached for 15 minutes,
encrypted with AES-256-GCM under a key kept beside it, both files `0600`.
Because the key sits next to the ciphertext, that guards against a secret being
picked up incidentally — a backup, a directory sync, a shared screen, a grep
across your home directory — not against someone who can already read your files
as you. The cache format is v2's, so both versions share it while both are
installed.

| Variable | Effect |
| --- | --- |
| `BINSQL_SECRET_TTL` | Cache lifetime in seconds. `0` resolves every time and writes nothing to disk. |
| `BINSQL_KEYVAULT_SUFFIX` | Key Vault DNS suffix, for sovereign clouds. |

Secrets are fetched through the Azure CLI, so this needs `az` and `az login`
just as `fedauth=` does. **Narrower than v2**, which linked the Azure SDK and
could also use a managed identity or an `AZURE_CLIENT_ID` service principal via
`BINSQL_AZURE_CREDENTIAL`. Shelling out to `az` keeps one Azure story rather
than two — `fedauth=` uses the same mechanism — but a CI job has no `az login`
to lean on, so see [Status](#status).

### The schema cache

Every level of the explorer costs a round trip, and against a remote SQL Server
four of them is the difference between opening a table and waiting to open one.
So each answer is written to `~/.cache/binsql/schema` and read back the next
time that node is expanded.

The cache is never trusted on its own. A hit is shown immediately **and** the
real query still runs; the answer is only applied when it differs from what was
shown. The common case is instant and silent, a schema that has actually
changed redraws itself a moment later, and there is no staleness window to
reason about. `⌃F5` forgets the entry first, so a refresh cannot be answered by
the thing it was pressed to get past.

Entries are keyed by the data source's name **and** its connection string, so
repointing a `local` at another server does not hand it the tree the old one
had, and two projects that both call something `local` do not share one. A file
that cannot be read, cannot be parsed or has gone stale is a miss rather than
an error: the worst a broken cache can do is cost the round trip it was there
to save.

Nothing in it is a secret — object names, not data — so it is plain JSON in the
cache directory rather than the encrypted secret cache above. Deleting the
whole thing costs one slow expansion.

| Variable | Effect |
| --- | --- |
| `BINSQL_SCHEMA_CACHE` | `0`, `off` or `false` turns it off, and every expansion is a round trip. |
| `XDG_CACHE_HOME` | Where it lives, if not `~/.cache`. |

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
  source refuses a write in one place, and both front ends inherit it.

### Look

**Panes and overlays follow binvim**, which shares the terminal. The body —
query buffer, result grid — is `#1e1e2e`; chrome — tree, tab strip, header and
status lines, every overlay — is `#181825`, so chrome reads as layered above
rather than painted in. binvim's chrome roles carry the same names and values,
and `theme.rs` is the only file that names a colour; everything else asks for a
role. Popups take binvim's form: title after a single dash in the top border, a
counter at the right end of it, `▌` down the left of the selected row. A scrim
blends the layout back while a modal is up, so the modal is plainly the thing
being talked to. Nerd Font glyphs for servers, databases, schemas, tables,
views, columns and keys.

**The header and status line follow Claude Code**, which is quieter: one mark in
its coral `#d97757`, then plain text separated by `·`. These started as binvim's
powerline segments with a chip naming the focused pane, which was a
mistranslation — binvim's chips announce a *mode*, binsql has none, and the
focused pane already says so with its border.

### Cancelling

`⌃C` calls off the running query. The console is yours again immediately; what
it costs the server depends on what the server offers. Postgres and MySQL are
told to stop — a second connection sends `pg_cancel_backend` or `KILL QUERY`,
the only way either of them hears it, since neither notices a client that has
stopped listening. SQL Server has no such statement and tiberius does not expose
TDS's attention signal, so the connection is dropped and replaced, which the
server reads as a disconnect and abandons the batch for. SQLite runs in this
process and has no server to call off.

A `Session` is a data source, not a database. Backends that cannot read across
their own databases on one connection — Postgres — grow a second connection
lazily when you expand a sibling catalog; the ones that can, do not.

## Tests

```sh
cargo test --workspace
```

An end-to-end test connects to a real SQLite database, walks the tree, runs a
query and renders the whole layout to a test backend, so the thing people look
at is asserted rather than assumed. Command mode is tested by running the built
binary as a subprocess and reading its stdout, its stderr and its exit code,
because those three are its whole interface.

The keychain round trip is ignored by default, because a plain `cargo test` has
no business writing to your credential store. Run it deliberately:

```sh
cargo test -p binsql-core --test keychain_roundtrip -- --ignored
```

`cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
warnings` and this suite run on every push and pull request, and again on a tag
before anything is built. The release workflow also runs by hand — all five
targets, nothing published — and refuses a tag that disagrees with `Cargo.toml`.

## Status

What works today is everything above. What has **not** been carried across from
v2 yet:

- **Managing data sources from the command line.** v2 had `binsql conn add`. In
  v3 a data source is added with `⌃N`, or by editing the config by hand — which
  now means knowing whether you meant the user file or a project's.
- **Managed identity and service-principal credentials** for Key Vault. The
  references work and so does the CLI credential; the other two arms of v2's
  chain are missing, which matters now that command mode is here and a CI job
  has no `az login` to lean on.
- Exporting a result set, editing values in the grid, query history, and
  filtering the tree.

## Licence

MIT.
