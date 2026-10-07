//! binsql's command mode: the same databases, the same guarantees, without the
//! terminal UI.
//!
//! This is what a script, a CI job or an agent talks to. The verbs are
//! deliberately few and the split between them is a safety boundary rather
//! than a convenience: `query` reads, `exec` writes, `inspect` describes.

mod args;
mod exec;
mod inspect;
mod query;
mod render;
mod source;

use std::io::{IsTerminal, Read, Write};

use binsql_core::{Backend, DataSource, Session, Value, Workspace};
use tokio_util::sync::CancellationToken;

use args::Args;
use render::{Format, Options};

/// Everything that can go wrong, split by whose fault it is: a usage mistake
/// exits 2 and points at the help, anything else exits 1.
#[derive(Debug)]
pub struct Failure {
    pub message: String,
    pub usage: bool,
}

pub fn usage(message: impl Into<String>) -> Failure {
    Failure {
        message: message.into(),
        usage: true,
    }
}

pub fn failed(message: impl Into<String>) -> Failure {
    Failure {
        message: message.into(),
        usage: false,
    }
}

type Result<T> = std::result::Result<T, Failure>;

/// Whether the arguments are a command-mode verb's.
///
/// Anything else is the TUI's, so `binsql eimskip/prod` still opens a data
/// source named on the command line rather than being rejected as a bad verb.
/// `source` is a command only with an operation after it: alone or with
/// options it opens a data source named `source`, as it always did.
pub fn is_command(args: &[String]) -> bool {
    match args {
        [verb, operation, ..] if verb == "source" => !operation.starts_with('-'),
        [verb, ..] => verb != "source" && binsql_core::RESERVED_NAMES.contains(&verb.as_str()),
        [] => false,
    }
}

/// Runs a command and returns the process exit code.
pub async fn main(args: Vec<String>) -> i32 {
    let verb = args.first().cloned().unwrap_or_default();
    let rest = args.into_iter().skip(1).collect();

    let outcome = match verb.as_str() {
        "query" => query::run(rest).await,
        "exec" => exec::run(rest).await,
        "inspect" => inspect::run(rest).await,
        "source" => source::run(rest).await,
        // Unreachable through `main`, which checks `is_command` first, but this
        // match and the reserved names in core have to agree, and this is
        // where that would show.
        other => Err(usage(format!("unknown command {other}"))),
    };

    match outcome {
        Ok(()) => 0,
        Err(failure) => {
            eprintln!("error: {}", failure.message);
            if failure.usage {
                // A pointer, not the whole help: the message above already says
                // what was wrong, and forty lines under it hide it.
                eprintln!("\nrun `binsql --help` for usage");
                return 2;
            }
            1
        }
    }
}

pub const HELP: &str = "\
COMMAND MODE
    binsql query \"<sql>\"       run a read-only statement and print the result
    binsql exec \"<sql>\"        run statements that change the database
    binsql inspect [table]     list tables, or describe one
    binsql source list         list the saved data sources
    binsql source show NAME    show one, found the way --conn finds it
    binsql source default [NAME]
                               show the default, or make NAME the default
    binsql source test [NAME]  connect to it, or to the default, and say how far
                               it got
    binsql source clear-cache  delete the cached Key Vault secrets
    binsql source add NAME     save a new data source
    binsql source edit NAME    change a saved one
    binsql source remove NAME --force
                               delete it, then its keychain secret

CONNECTION
    -c, --conn NAME       a saved data source, `folder/name` or a bare name
    -D, --dsn STRING      a connection string, used instead of a saved one
    -d, --driver NAME     sqlite | postgres | mssql | mysql (default: inferred)

    With none of these, the `default` data source is opened. BINSQL_CONN,
    BINSQL_DSN and BINSQL_DRIVER say the same things through the environment.

OUTPUT
    -o, --format NAME     table (default), json, jsonl, csv, tsv, vertical,
                          markdown, raw, none
        --pretty          indent JSON
        --no-header       leave out the header row
        --no-footer       leave out the trailing row count

QUERY
    -f, --file FILE       read the SQL from a file, or from stdin for `-`
        --arg VALUE       fill the next ? placeholder; repeat, in order
        --limit N         stop after N rows (default: all of them)
        --allow-write     permit a statement that writes (prefer `exec`)
        --plan            print the estimated plan of one read instead of running
                          it; `-o raw` for the bare document

EXEC
    -f, --file FILE       read the script from a file, or from stdin for `-`
        --arg VALUE       fill the next ? placeholder; repeat, in order
        --dry-run         run the batch inside a transaction, then roll it back
        --tx / --no-tx    force one transaction around the batch, or none
        --force           permit UPDATE/DELETE with no WHERE, and DROP/TRUNCATE

INSPECT
        --catalog NAME    the database to read (default: the connected one)
        --schema NAME     the schema to read
        --columns         every table and view's columns, one row each, with
                          the catalog, schema, object and kind they belong to;
                          a NAME after it is matched exactly, never split on
                          dots, with the schema taken from --schema alone

SOURCE
    One row per data source: name, scope, driver, dsn, readonly, open_on_start,
    default, description, shadowed. A keychain:// or keyvault:// reference
    prints as written; a connection string has its password masked. Only
    source test connects, so only the OUTPUT flags apply.

    source default alone prints the default's row, or nothing when none is
    set. With a NAME it writes the default into the file that source is in.
    --scope user|project   write it into that file instead
    The project file's default wins over yours, so writing your config while
    the project sets a different default is refused: pass --scope project.

    source test resolves the connection string and connects, then prints one
    row: name, ok, stage, elapsed_ms, error. stage is where it stopped —
    secret (the vault or keychain), token (Azure AD) or connect — and is null,
    as error is, when it got through. It exits 0 when it connects, and 1 with
    the row still printed when it does not; the error has the password masked.
    --fresh                skip the cached secret and refresh it
    source clear-cache deletes every cached secret and the key that protects
    them. BINSQL_SECRET_TTL=0 keeps secrets out of the cache for every command.

    source add saves a new data source and source edit changes one, each
    printing its row. Folder and name are written folder/name. A connection
    string comes on stdin or from a variable, never on the command line:
    --dsn-stdin            read it from stdin
    --dsn-env VAR          read it from the variable VAR
    --dsn VALUE            a sqlite path or a secret reference only
    -d, --driver NAME      the driver (default: inferred, or kept on edit)
    --description TEXT     a note shown beside it
    --readonly / --no-readonly
    --open-on-start / --no-open-on-start
    --scope user|project   the file to save it in (default: its own file, and
                           a new one in the project's when there is one)
    --no-keychain          keep the string in your config rather than the
                           keychain; refused in the project file
    --rename NEW           source edit: save it under a new name
    What edit is not given stays as it was, the connection string included.

    source remove deletes a data source from every file that holds it, then
    its keychain secret, and prints the row it had. It needs --force, since
    the secret cannot be brought back; when the secret cannot be deleted it
    exits 1 with the data source already gone.

BIND VALUES
    An --arg is text unless it says otherwise: int:42, float:1.5, bool:true,
    null:, json:{\"a\":1} — or str: for text that begins with one of those.

EXAMPLES
    binsql query \"select * from users limit 20\" -o json --pretty
    binsql query \"select * from users where id = ?\" --arg int:7
    binsql query -f report.sql --conn eimskip/prod -o csv > report.csv
    binsql exec -f migration.sql --dry-run
    binsql inspect --conn scratch
    binsql inspect users
    binsql source list -o json
";

/// The flags every command shares. Kept in one list so `--conn` means the same
/// thing everywhere and a new verb cannot quietly spell it differently.
const SHARED_VALUES: &[&str] = &[
    "conn", "c", "dsn", "D", "driver", "d", "format", "o", "catalog", "schema",
];
const SHARED_SWITCHES: &[&str] = &["pretty", "no-header", "no-footer"];

pub fn parse(args: Vec<String>, values: &[&str], switches: &[&str]) -> Result<Args> {
    let values: Vec<&str> = SHARED_VALUES.iter().chain(values).copied().collect();
    let switches: Vec<&str> = SHARED_SWITCHES.iter().chain(switches).copied().collect();
    Args::parse(args, &values, &switches)
}

pub fn output(args: &Args) -> Result<Options> {
    let name = args.value(&["format", "o"]).unwrap_or("table");
    let format = Format::parse(name)
        .ok_or_else(|| usage(format!("unknown format {name} — one of: {}", Format::NAMES)))?;

    Ok(Options {
        format,
        pretty: args.is_set(&["pretty"]),
        header: !args.is_set(&["no-header"]),
        footer: !args.is_set(&["no-footer"]),
    })
}

/// Opens the data source the flags name.
///
/// A saved connection wins over a DSN, and the environment fills in whatever
/// the flags left out — the order a script expects, where the command line
/// overrides what the shell already set.
pub async fn connect(args: &Args) -> Result<Session> {
    // A Workspace, so command mode sees the same project `.binsql.json` the
    // TUI does when it is run from inside a repository.
    let config =
        Workspace::load().map_err(|error| failed(format!("loading connections: {error}")))?;

    let named = args
        .value(&["conn", "c"])
        .map(str::to_string)
        .or_else(|| std::env::var("BINSQL_CONN").ok());
    let dsn = args
        .value(&["dsn", "D"])
        .map(str::to_string)
        .or_else(|| std::env::var("BINSQL_DSN").ok());
    let driver = match args
        .value(&["driver", "d"])
        .map(str::to_string)
        .or_else(|| std::env::var("BINSQL_DRIVER").ok())
    {
        Some(name) => Some(Backend::parse(&name).map_err(|error| usage(error.to_string()))?),
        None => None,
    };

    let (name, source) = match (named, dsn) {
        (Some(named), _) => {
            let id = config
                .resolve(&named)
                .ok_or_else(|| usage(format!("no saved data source named {named}")))?;
            let source = config
                .get(&id)
                .cloned()
                .ok_or_else(|| usage(format!("no saved data source named {named}")))?;
            (id, source)
        }
        (None, Some(dsn)) => {
            let backend = driver.or_else(|| Backend::infer(&dsn)).ok_or_else(|| {
                usage("could not tell which driver that connection string needs — pass --driver")
            })?;
            (
                "--dsn".to_string(),
                DataSource {
                    backend,
                    dsn,
                    description: String::new(),
                    read_only: false,
                    open_on_start: false,
                },
            )
        }
        (None, None) => default_source(&config)?,
    };

    Session::open(name.clone(), source)
        .await
        .map_err(|error| failed(format!("connecting to {name}: {error}")))
}

/// The data source the config names as its default, and its qualified name.
pub fn default_source(config: &Workspace) -> Result<(String, DataSource)> {
    let named = config.default.clone().ok_or_else(|| {
        usage("no data source given, and none is the default — pass --conn or --dsn")
    })?;
    // Through `resolve`, so a `default` of `prod` finds `eimskip/prod`
    // exactly as `--conn prod` does — and says which two it found when
    // that name has stopped meaning one thing.
    let id = config.resolve(&named).ok_or_else(|| {
        failed(
            config
                .unresolved_default()
                .unwrap_or_else(|| format!("the default data source {named} is not in the config")),
        )
    })?;
    let source = config
        .get(&id)
        .cloned()
        .ok_or_else(|| failed(format!("the default data source {id} is not in the config")))?;
    Ok((id, source))
}

/// A token that a ⌃C from the terminal cancels, so a long query stops the way
/// it does in the TUI rather than leaving the server working on an answer the
/// process is no longer there to read.
pub fn cancel_on_interrupt() -> CancellationToken {
    let cancel = CancellationToken::new();
    let triggered = cancel.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            triggered.cancel();
        }
    });
    cancel
}

/// The SQL a command was given: an argument, a file, or stdin.
///
/// Reading stdin when it is a pipe and there is no argument is what makes
/// `cat query.sql | binsql query` work without a flag saying so.
pub fn read_sql(args: &Args) -> Result<String> {
    if let Some(path) = args.value(&["file", "f"]) {
        if path == "-" {
            return read_stdin();
        }
        return std::fs::read_to_string(path)
            .map_err(|error| failed(format!("reading {path}: {error}")));
    }

    let positional = args.positional();
    if positional.len() > 1 {
        return Err(usage(
            "more than one statement was given as an argument — quote the whole script as one",
        ));
    }
    if let Some(sql) = positional.first() {
        if sql == "-" {
            return read_stdin();
        }
        return Ok(sql.clone());
    }

    if std::io::stdin().is_terminal() {
        return Err(usage(
            "no SQL given — pass it as an argument, with --file, or on stdin",
        ));
    }
    read_stdin()
}

/// The values for the statement's `?` placeholders, one per `--arg`, in the
/// order given.
///
/// Text unless a prefix says otherwise — `int:`, `float:`, `bool:`, `json:`,
/// `null:`, or `str:` for text that begins with one of those. A colon after
/// anything else is part of the value, so `--arg 12:30` is the text it looks
/// like.
pub fn bind_values(args: &Args) -> Result<Vec<Value>> {
    args.values("arg").into_iter().map(bind_value).collect()
}

fn bind_value(raw: &str) -> Result<Value> {
    let Some((prefix, rest)) = raw.split_once(':') else {
        return Ok(Value::Text(raw.to_string()));
    };
    let invalid = |kind: &str| usage(format!("--arg {raw}: {rest:?} is not {kind}"));

    match prefix.to_ascii_lowercase().as_str() {
        "int" => rest
            .parse::<i64>()
            .map(Value::Int)
            .map_err(|_| invalid("an integer")),
        "float" => rest
            .parse::<f64>()
            .map(Value::Float)
            .map_err(|_| invalid("a number")),
        "bool" => match rest {
            "1" | "t" | "T" | "TRUE" | "true" | "True" => Ok(Value::Bool(true)),
            "0" | "f" | "F" | "FALSE" | "false" | "False" => Ok(Value::Bool(false)),
            _ => Err(invalid("true or false")),
        },
        "null" => Ok(Value::Null),
        "json" => serde_json::from_str::<serde_json::Value>(rest)
            .map(|_| Value::Json(rest.to_string()))
            .map_err(|_| invalid("JSON")),
        "str" | "string" => Ok(Value::Text(rest.to_string())),
        _ => Ok(Value::Text(raw.to_string())),
    }
}

fn read_stdin() -> Result<String> {
    let mut buffer = String::new();
    std::io::stdin()
        .read_to_string(&mut buffer)
        .map_err(|error| failed(format!("reading stdin: {error}")))?;
    Ok(buffer)
}

/// Writes the output, treating a closed pipe as the end of the job rather than
/// as a failure — `binsql query … | head` is a normal thing to type.
pub fn print(text: &str) -> Result<()> {
    if text.is_empty() {
        return Ok(());
    }
    let mut out = std::io::stdout();
    match out.write_all(text.as_bytes()).and_then(|()| out.flush()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(failed(format!("writing output: {error}"))),
    }
}

/// Says something to the operator without putting it where a parser will find
/// it. Structured output goes to stdout alone.
pub fn note(options: &Options, message: &str) {
    if options.format == Format::None {
        return;
    }
    eprintln!("{message}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_verbs_are_the_reserved_names() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        for verb in ["query", "exec", "inspect"] {
            assert!(is_command(&args(&[verb])), "{verb}");
        }
        assert!(is_command(&args(&["source", "list"])));
        for list in [
            &["source"][..],
            &["source", "-d", "postgres"],
            &["source", "--help"],
            &["source", "--", "x"],
            &["eimskip/prod"],
            &["--"],
            &[],
        ] {
            assert!(!is_command(&args(list)), "{list:?}");
        }
    }
}
