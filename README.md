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
binsql -- query                 # a saved data source named like a verb
```

A first argument that names a verb — `query`, `exec`, `inspect` — is
[command mode](#command-mode); anything else is a data source to open. Anything
binsql can neither find among the saved ones nor read as a connection string is
an error naming both ways out, since a mistyped verb lands there as readily as a
bad DSN. `-h`, `-V`, `--capabilities` and `--debug-keys` print and stop. After
`--` the next argument is always a data source, even one named like a verb. A
new top-level data source cannot take a verb's name, though one inside a folder
can, and one saved under it before stays editable.

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
binsql inspect --columns -o jsonl                    # every column of everything
binsql inspect --columns artist -o json              # one object, exact name
binsql inspect --definitions -o jsonl                # how each was defined
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
| `--connect-timeout-ms N` | give up when resolving the connection string and connecting have not finished `N` ms after binsql began loading the config |

`BINSQL_CONN`, `BINSQL_DSN`, `BINSQL_DRIVER` and `BINSQL_CONNECT_TIMEOUT_MS`
say the same things through the environment, and a flag beats the variable.

The connect budget is off by default, and `0` turns it off: binsql then waits
as long as the driver, or `az`, does. With one, running out is exit 1 and
`connect timeout: no connection to NAME within N ms, while …` names the step
it was on — resolving the connection string, or connecting. The drivers' own
timeouts are left as they are, so the first to run out wins. Loading the
config counts against the budget but is never cut short. The budget ends
once connected: running the statements, and any further connection
`inspect --catalog` opens, are outside it.

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

`--timeout-ms N` cancels the statement `N` ms after the connection opens, and
exits `1` with `timed out after N ms`, category `timeout`. Connecting is not
counted: `--connect-timeout-ms` covers that. A cancel the database has not
acted on 2 s after the deadline is left behind, and binsql exits all the same.
A read says nothing was changed by binsql; a write under `--allow-write` says
whether it took effect is unknown, because a timeout is not proof of a
rollback. `0`, or no flag, is no limit.

On SQLite, a statement that returns nothing until it ends — an aggregate, an
`INSERT … SELECT` — does not stop at the cancel, since SQLite's worker notices
it only when it next hands over a row. binsql exits at the deadline all the
same, and the statement and its lock end with the process. A wait for another
connection's lock counts against the deadline, but the driver stops waiting
after 5 s on its own, with `database is locked`, category `database`: a
deadline longer than that is not what ends the wait.

`--require-rows` turns an empty result into a failure: the result prints as it
would without the flag, and then the query exits `1` with `the query returned no
rows (--require-rows)`, category `assertion`. A check reads as a predicate
`SELECT` that returns a row only when the condition holds. An aggregate such as
`COUNT(*)` always returns a row, a zero included, so it never fails the check.
A write under `--allow-write` counts the rows it returns with `RETURNING`; on
SQL Server, rows from `OUTPUT` or `EXEC` are not kept, so such a write always
fails the check.

```sh
binsql query --require-rows -o none \
  "SELECT 1 FROM orders WHERE status = 'stuck' LIMIT 1" && echo "stuck orders"
```

`--plan` prints the plan the database estimates for one `SELECT`, `WITH`,
`VALUES` or `TABLE` statement, and does not run it. A write is refused even
with `--allow-write` — drop `--plan` to run it — and so are `--arg` and a
statement that is already an `EXPLAIN`, `SHOW` or `PRAGMA`. A read-only data
source can plan.

| | Sent | Returns |
| --- | --- | --- |
| SQLite | `EXPLAIN QUERY PLAN` | rows of `id, parent, notused, detail` |
| PostgreSQL | `EXPLAIN (FORMAT JSON)` | one JSON value |
| MySQL | `EXPLAIN FORMAT=JSON` | one JSON value as text |
| SQL Server | `SET SHOWPLAN_XML ON` around the statement | one XML value |

The plan comes back as a result set and goes through `-o` like any other, so
`-o raw` prints the bare document for `jq` or an XML tool. It is an estimate:
no row counts or timings from a run. SQL Server needs the `SHOWPLAN`
permission (`GRANT SHOWPLAN TO <user>`), and says so when it is missing.

```sh
binsql query --plan "SELECT * FROM orders WHERE customer_id = 31" -o raw | jq .
```

`--stream` writes each record as its row arrives instead of holding the whole
result first, so a large export starts at once and holds a few hundred rows at
a time. It writes `-o jsonl`, `-o csv` and `-o tsv`, a record per row, and
refuses any other format, since a table, JSON array or plan needs every row
before its first byte. It reads only: a write is refused even with
`--allow-write`, and so is `--plan`.

The records are the same bytes the query prints without the flag, but a
failure part-way leaves the records already written in place and exits `1`
after them: only exit `0` means the output is complete. A query cut short by
`--limit` says `stopped at --limit N; more rows were available` on stderr. A
reader that goes away — `| head` — stops the query and exits `0`, and since
how many rows there were is then unknown, `--require-rows` is not checked; a
query that had already failed, or a ⌃C, still exits `1`. The limit bounds the
rows binsql holds, not the work the database does to produce them or the size
of any one row. `--require-rows` and `--timeout-ms` otherwise apply as they do
without it. A reader that stops reading but keeps the pipe open holds the query
where it is; at the deadline or a ⌃C, binsql waits 2 s for the output to drain
and then exits without it, and the last record may be cut short.

```sh
binsql query --stream -o csv "SELECT * FROM events" | gzip > events.csv.gz
```

`exec` takes `-f, --file FILE`, and:

| | |
| --- | --- |
| `--dry-run` | run the batch in a transaction and roll it back — it really executes, and nothing is kept |
| `--tx` / `--no-tx` | force one transaction around the batch, or none |
| `--force` | permit `UPDATE`/`DELETE` with no `WHERE`, and `DROP`/`TRUNCATE` |
| `--timeout-ms N` | stop the batch `N` ms after the connection opens; `0`, or no flag, is no limit |

A batch of two or more is one transaction by default; a lone statement is not
worth wrapping, and `--tx` is how to say it should be anyway. Without `--force`,
the two statements most likely to be a mistake are refused before they are sent.
`--dry-run` is transactional everywhere except MySQL DDL, which MySQL commits as
it runs — binsql says so rather than letting a dry run imply otherwise.

When a transactional batch fails, binsql says how the transaction ended: rolled
back, so nothing was kept; unknown, when the rollback or the commit itself
failed; or never begun, when the batch was turned away before it ran.

`--timeout-ms` covers the whole batch, as it covers `query`'s one statement:
category `timeout`, exit `1`, and a cancel not acted on within 2 s left
behind. No statement starts after the deadline. Without a transaction, binsql
says which statement was running, or which would have started next, and how
many before it ran and were kept. A transaction cut off is `unknown`: binsql
sends no `COMMIT` after the deadline, but one already in flight may still
land, and a `--dry-run` cannot say whether its rollback completed.

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
| `--columns` | every table and view's columns in one call |
| `--definitions` | every table and view's own definition text in one call |
| `--timeout-ms N` | give up `N` ms after the connection opens; `0`, or no flag, is no limit |

A table may carry its own schema — `binsql inspect dbo.orders` — which wins over
`--schema`, being the more specific of the two. Either way the name is matched
against what the server says it has rather than pasted into a query, so a
misspelling comes back as binsql saying there is no such table instead of as a
syntax error from the database.

`--columns` answers with one row per column — `catalog, schema, object, kind,
column, type, nullable, default, primary_key` — so an agent learns the whole
schema in one call rather than one per table. Objects come sorted by schema
(none first), then name, then kind, comparing bytes, the same on every backend;
columns keep the order they were declared in. `schema` is null on SQLite and
MySQL, which have no schema level. A table or view that comes back with no
columns still gets a row, everything past its identity null, and a note on
stderr. Read it as JSON or JSONL: CSV and TSV print a null and an empty string
the same way. SQL Server's `type` is the bare type name, without its length or
precision, and on SQLite generated columns and the hidden columns of a virtual
table are not listed.

`--timeout-ms` covers every read `inspect` makes after connecting, the
connection `--catalog` opens included. Running out prints nothing on stdout and
exits 1 with `timed out after N ms`; `inspect` only reads, so nothing was
changed. ⌃C stops it as it always has.

A name after `--columns` is an identity rather than a guess: it is matched
exactly, case included, and never split on dots, so a table called `a.b` can be
named and its schema comes from `--schema` alone. No match exits `1`, naming any
objects that differ only in case; a name found in more than one schema exits
`2` until `--schema` says which. Plain `inspect NAME` still matches loosely, as
it always has.

`--definitions` selects objects the way `--columns` does, name and order
included, and answers with one row per object — `catalog, schema, object, kind,
form, definition`. `definition` is the text exactly as the server stores or
prints it, never reformatted and never built from the columns; `form` says what
that text is:

| `form` | `definition` |
| --- | --- |
| `create` | a complete `CREATE` statement |
| `query` | a view's `SELECT` alone, with no `CREATE` around it |
| `unsupported` | null: binsql does not read definitions for this backend and kind |
| `withheld` | null: the server has the object but gave no text |

Rows without text still exit `0`, with one line on stderr counting them by form.
Any failure to read one prints nothing on stdout and exits `1`, naming the
object. It cannot be given with `--columns`, since each has its own rows; the
two together exit `2`.

| Backend | Table | View |
| --- | --- | --- |
| SQLite | `create` | `create` |
| PostgreSQL | `unsupported` | `query` |
| MySQL | `unsupported` | `unsupported` |
| SQL Server | `unsupported` | `unsupported` |

This is context for whoever reads it, not an export to restore from: there is
no dependency order, no grants and no ownership. SQLite's table text has its
inline constraints but not the indexes or triggers created beside it, and is
kept as it was written — after an `ALTER TABLE … RENAME` it may read as SQLite
rewrote it, with SQLite's own spelling of the opening `CREATE TABLE`.
PostgreSQL keeps no `CREATE` text for a table, so its tables stay
`unsupported`; a view's `query` is the server rebuilding the view from its
parsed form (`pg_get_viewdef`), so its layout is the server's and comments
written in the view are gone.

Exit codes are `0` for success, `1` for a database that said no, and `2` for a
usage mistake, so a script can tell "you asked wrong" from "it did not work".
`⌃C` cancels the running query the same way it does in the TUI.

### Structured errors

`--error-format json`, or `BINSQL_ERROR_FORMAT=json`, turns a failure into one
compact JSON line on stderr in place of the `error: …` text, so an agent can
branch on what went wrong without parsing a sentence. The flag beats the
variable, and an empty variable counts as unset. Stdout and the exit code are
the same either way.

```json
{"type":"error","schema":1,"exit":1,"category":"database","phase":"execute","message":"no such table: artists"}
```

| Field | |
| --- | --- |
| `exit` | the exit code the process ends with |
| `category` | `usage`, `source`, `config`, `secret`, `connect`, `connect-timeout`, `timeout`, `refused`, `database`, `assertion`, `cancelled`, `io` or `other` |
| `phase` | `args`, `input`, `config`, `connect`, `prepare`, `execute` or `output` |
| `message` | what went wrong, in one sentence |
| `reason` | why, for a Key Vault or Azure AD failure: `az-missing`, `az-unauthenticated`, `vault-forbidden`, `secret-not-found`, `vault-not-found` or `azure-ad-token` |
| `code` | the database's own code for the failure, as a string, when its driver gives one |
| `hint` | the next step to try, when binsql knows one |
| `detail` | what the Azure CLI said, or the database's answer to a timeout's cancel, redacted like `message` and cut at 1000 characters |
| `statement` | the 1-based statement that failed or was refused, when there is one |
| `completed` | how many statements before it were kept, under `exec --no-tx` |
| `transaction` | how a transactional `exec` ended: `rolled_back`, `unknown` or `none`; `unknown` too for a transactional `exec`, or a write `query --allow-write` ran, past `--timeout-ms` |

`category` says what kind of thing failed — a missing saved data source is
`source`, a vault or keychain that would not hand over a secret is `secret`, a
statement turned away by `query` or by `exec` without `--force` is `refused`,
an empty result under `--require-rows` is `assertion`, a connect budget run
out is `connect-timeout`, a statement, batch or `inspect` past `--timeout-ms` is `timeout` —
and `phase` says how far binsql had got. A note that would have gone to stderr
comes as `{"type":"notice","schema":1,"message":…}`, and `-o none` still hides
it. A bad `--error-format` value is reported as text, since there is no
agreed format yet to report it in.

`schema` is the record's revision. Adding a field or a category keeps it at
`1`; renaming or removing one, or changing what one means, raises it. Read a
category you do not know as `other`.

A `keyvault://` secret that would not resolve keeps its message to the
reference, and puts the rest in fields of its own:

```json
{"type":"error","schema":1,"exit":1,"category":"secret","phase":"connect","message":"connecting to prod: reading keyvault://kv-prd/dsn","reason":"az-unauthenticated","hint":"no usable Azure credential — run `az login`","detail":"Please run 'az login' to setup account."}
```

A `fedauth=` connection whose Azure AD token could not be had is category
`connect` with reason `azure-ad-token`. A reason you do not know means the
same as none.

`code` is only there when the driver hands binsql the code as a field of its
own; it is never read out of the message. What it holds depends on the
database:

| Backend | `code` | Example |
| --- | --- | --- |
| PostgreSQL | the SQLSTATE | `23505`, a unique violation |
| MySQL | the SQLSTATE, not the server's error number | `23000`, for error 1062 |
| SQLite | the extended result code, in decimal | `2067`, a UNIQUE constraint |
| SQL Server | the error number, in decimal | `2627`, a unique key violation |

A failure binsql raises itself, such as a refusal, has no `code`.

`transaction` is `rolled_back` when a statement failed and the rollback went
through, so nothing was kept; `unknown` when the rollback, the commit, or the
rollback that ends a `--dry-run` failed, so the database alone knows; and `none`
when no statement was sent. A batch that could have committed part of itself —
one with its own `COMMIT`, `SET` or other control statement, or DDL on MySQL —
is `unknown` rather than `rolled_back`, since the rollback may have had nothing
left to undo. A failed commit has no `statement`. `exec --no-tx`
has no `transaction`; it has `completed`.

The message is redacted: a stored connection string, the password in
`scheme://user:pass@`, the value of `password=`, `pwd=` and `accesstoken=`, and
anything shaped like a JWT become `****`. The SQL is left out — the text form
names the statement, the record only counts it. A message from the server is
passed on as it came, though, and can quote a value from the data.

### Capabilities

`binsql --capabilities` prints one compact JSON line saying what this binary
can do, so an agent can check an installed binsql before it relies on a flag,
without parsing `--help`. It reads no config, resolves no secret and connects
to nothing, and it stops there, as `--version` does.

```json
{"type":"capabilities","schema":1,"version":"3.0.0","backends":["sqlite","postgres","mssql","mysql"],"commands":{"inspect":{"options":["--conn","-c",…],"switches":["--pretty",…,"--columns"]},…},"formats":["table","json",…],"error_formats":["text","json"],"error_schema":1,"exit_codes":{"0":"success","1":"failure","2":"usage"}}
```

| Field | |
| --- | --- |
| `version` | the binary's version, as `--version` prints it |
| `backends` | the databases it was built to reach |
| `commands` | `query`, `exec`, `inspect` and `source`, each with the `options` that take a value and the `switches` that do not, spelled as typed, a short alias as its own entry; `source` also lists its `operations` |
| `formats` | what `-o` takes, by canonical name; the aliases still work |
| `error_formats` | what `--error-format` takes |
| `error_schema` | the revision of the [error record](#structured-errors) |
| `exit_codes` | what each exit code means |

`schema` is the manifest's revision, kept the same way as the error record's:
adding a field keeps it, renaming or removing one raises it.

### Data sources

`binsql source list` prints every saved data source, user and project alike, and
`binsql source show NAME` prints one, found the way `--conn` finds it. Each row
is `name, scope, driver, dsn, readonly, open_on_start, default, description,
shadowed`. A `keychain://` or `keyvault://` reference prints as written, and a
connection string kept in the config has its password masked: nothing is
resolved and nothing connects, so of the shared flags only the output ones
apply. `source test`, below, is the one that does connect.

`shadowed` marks a data source a command has taken the bare name of — a
top-level `query`, or `team/inspect` when it is the only `inspect` — and a note
on stderr says how to open it instead. `binsql source` alone, or with only
options after it, still opens a saved data source named `source`; it is a
command only with an operation after it.

`binsql source default` prints the default data source's row, or nothing when
none is set. `binsql source default NAME` makes NAME the default, writing it
into the file that data source is saved in; `--scope user` or `--scope project`
writes it into that file instead. A project's default wins over yours, so
setting it in your config while the project file names a different one would
change nothing, and is refused with a hint to pass `--scope project`.

`binsql source test NAME` resolves the data source's connection string and
connects, then prints one row: `name, ok, stage, elapsed_ms, error`. With no
NAME it tries the default, as a command with no `--conn` would. `stage` says
where it stopped — `secret` (the Key Vault or keychain lookup), `token` (the
Azure AD token for SQL Server) or `connect` — and is null, as `error` is, when
it got through. It exits 0 when it connects; otherwise the row is still printed
and it exits 1, with the password masked in the error. `--fresh` skips the
cached secret and caches what it fetches, for a secret rotated since it was
cached. `binsql source clear-cache` deletes every cached secret and the key that
protects them; `BINSQL_SECRET_TTL=0` keeps them out of the cache altogether.

`binsql source add NAME` saves a new data source and `binsql source edit NAME`
changes a saved one; both print the saved row. A folder goes in the name, as
`team/prod`. The connection string never goes on the command line, where it
would sit in your shell history: `--dsn-stdin` reads it from stdin and
`--dsn-env VAR` from a variable, while `--dsn` takes only a sqlite path or a
`keyvault://` or vault URL reference, which hold no secret. A connection string
is filed in the OS keychain and the config holds `keychain://NAME`;
`--no-keychain` keeps it in your config instead, and is refused for the project
file, which is meant to be shared. The other flags are `-d/--driver`,
`--description`, `--readonly`/`--no-readonly`,
`--open-on-start`/`--no-open-on-start` and `--scope user|project`: a new data
source goes into the project file when there is one, and an edited one stays in
its own unless `--scope` moves it. `source edit` changes only what it is given,
and `--rename NEW` saves it under a new name, carrying its keychain entry with it.

```sh
printf %s "$PROD_DSN" | binsql source add eimskip/prod --dsn-stdin --readonly
binsql source add scratch --dsn ./scratch.db --scope user
binsql source edit eimskip/prod --description "read replica" --rename eimskip/replica
```

`binsql source remove NAME --force` deletes a data source from every file that
holds it, then its keychain secret, and prints the row it had. Without
`--force` it refuses, since a deleted secret cannot be brought back. The secret
goes only once the config is written; if it cannot be deleted, the command
exits 1 with the data source already gone and says what is left in the
keychain.

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

Under `--connect-timeout-ms`, fetching the token is part of connecting. If
the budget runs out while `az` is still working, binsql kills `az` — on unix,
with everything it started — and the error ends `while connecting`.
The token is fetched once: the connection that replaces one lost to a
cancelled statement reuses it, and `inspect --catalog` reads every catalog
over the first connection.

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
| `BINSQL_SECRET_TTL` | Cache lifetime in seconds. `0` resolves every time and writes nothing to disk, which is also how to pick up a secret rotated within the cache lifetime. |
| `BINSQL_KEYVAULT_SUFFIX` | Key Vault DNS suffix, for sovereign clouds. |

Secrets are fetched through the Azure CLI, so this needs `az` and `az login`
just as `fedauth=` does. **Narrower than v2**, which linked the Azure SDK and
could also use a managed identity or an `AZURE_CLIENT_ID` service principal via
`BINSQL_AZURE_CREDENTIAL`. Shelling out to `az` keeps one Azure story rather
than two — `fedauth=` uses the same mechanism.

A CI job, a container or an agent gets its login the same way, with one of the
Azure CLI's non-interactive forms run before binsql:

```sh
az login --service-principal -u "$AZURE_CLIENT_ID" --password="$AZURE_CLIENT_SECRET" --tenant "$AZURE_TENANT_ID"
az login --service-principal -u "$AZURE_CLIENT_ID" --federated-token "$TOKEN" --tenant "$AZURE_TENANT_ID"
az login --identity    # a managed identity, on Azure compute
```

Both Key Vault references and `fedauth=` then use that login. Without one,
`az` fails with its own error, which binsql passes on. `az` gets no stdin, so
it cannot stop at a prompt, but on its own it has no deadline: an `az` that
hangs holds binsql with it.

`--connect-timeout-ms` is that deadline. Reading the secret counts against
the budget; if it runs out while `az` is still working, binsql kills `az` — on
unix, with everything it started — caches nothing, and the error ends
`while resolving the connection string`. A reference still in the cache makes
no call at all. `inspect --catalog` never reads the vault again: a further
catalog's connection reuses the string the first one resolved.

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
process and has no server to call off: its statement runs on a worker thread
that notices the cancel when it next hands over a row. One that returns nothing
until it ends, such as a `count(*)` or an `INSERT … SELECT`, runs on, holding
its lock, until binsql quits.

The same holds inside a transaction, as `exec` runs a batch. The transaction
names its server session as it begins, so a cancelled statement is stopped on
the server first and the rollback follows once it has let go; otherwise the
rollback would queue behind a statement still running.

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

- **Managed identity and service-principal credentials** for Key Vault, as
  binsql's own. v2 linked them in; v3 reaches the same identities through
  `az login --identity` and `az login --service-principal`, run first (see
  [Azure Key Vault references](#azure-key-vault-references)).
- Exporting a result set, editing values in the grid, query history, and
  filtering the tree.
- **Definitions** on MySQL and SQL Server: `inspect --definitions` marks their
  tables and views `unsupported` for now.

## Licence

MIT.
