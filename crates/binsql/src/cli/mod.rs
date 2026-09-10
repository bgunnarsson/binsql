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

use std::io::{IsTerminal, Read, Write};

use binsql_core::{Backend, DataSource, Session, Workspace};
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

const VERBS: [&str; 3] = ["query", "exec", "inspect"];

/// Whether the first argument names a command-mode verb.
///
/// Anything else is the TUI's, so `binsql eimskip/prod` still opens a data
/// source named on the command line rather than being rejected as a bad verb.
pub fn is_verb(name: &str) -> bool {
    VERBS.contains(&name)
}

/// Runs a command and returns the process exit code.
pub async fn main(args: Vec<String>) -> i32 {
    let verb = args.first().cloned().unwrap_or_default();
    let rest = args.into_iter().skip(1).collect();

    let outcome = match verb.as_str() {
        "query" => query::run(rest).await,
        "exec" => exec::run(rest).await,
        "inspect" => inspect::run(rest).await,
        // Unreachable through `main`, which checks `is_verb` first, but the
        // two lists have to agree and this is where that would show.
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
        --limit N         stop after N rows (default: all of them)
        --allow-write     permit a statement that writes (prefer `exec`)

EXEC
    -f, --file FILE       read the script from a file, or from stdin for `-`
        --dry-run         run the batch inside a transaction, then roll it back
        --tx / --no-tx    force one transaction around the batch, or none
        --force           permit UPDATE/DELETE with no WHERE, and DROP/TRUNCATE

INSPECT
        --catalog NAME    the database to read (default: the connected one)
        --schema NAME     the schema to read

EXAMPLES
    binsql query \"select * from users limit 20\" -o json --pretty
    binsql query -f report.sql --conn eimskip/prod -o csv > report.csv
    binsql exec -f migration.sql --dry-run
    binsql inspect --conn scratch
    binsql inspect users
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
        (None, None) => {
            let named = config.default.clone().ok_or_else(|| {
                usage("no data source given, and none is the default — pass --conn or --dsn")
            })?;
            // Through `resolve`, so a `default` of `prod` finds `eimskip/prod`
            // exactly as `--conn prod` does — and says which two it found when
            // that name has stopped meaning one thing.
            let id = config.resolve(&named).ok_or_else(|| {
                failed(config.unresolved_default().unwrap_or_else(|| {
                    format!("the default data source {named} is not in the config")
                }))
            })?;
            let source = config.get(&id).cloned().ok_or_else(|| {
                failed(format!("the default data source {id} is not in the config"))
            })?;
            (id, source)
        }
    };

    Session::open(name.clone(), source)
        .await
        .map_err(|error| failed(format!("connecting to {name}: {error}")))
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
