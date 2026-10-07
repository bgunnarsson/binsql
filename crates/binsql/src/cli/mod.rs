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
use std::sync::OnceLock;

use binsql_core::{Backend, DataSource, Session, Value, Workspace};
use tokio_util::sync::CancellationToken;

use args::Args;
use render::{Format, Options};

/// Everything that can go wrong, split by whose fault it is: a usage mistake
/// exits 2 and points at the help, anything else exits 1.
///
/// The category and phase are set where the failure is raised, for
/// `--error-format json`; they never change the exit code.
#[derive(Debug)]
pub struct Failure {
    pub message: String,
    pub usage: bool,
    pub category: Category,
    pub phase: Phase,
    /// The 1-based position of the statement that failed.
    pub statement: Option<usize>,
    /// How many statements had already run, with no transaction to undo them.
    pub completed: Option<usize>,
    /// What became of the transaction a batch ran in.
    pub transaction: Option<Transaction>,
    /// Lines text mode prints indented under the message. JSON leaves them
    /// out: they quote the SQL, which `statement` stands in for.
    pub context: Vec<String>,
    /// Why it failed, in a word from the contract's list.
    pub reason: Option<&'static str>,
    /// The message taken apart, and the database's code, when a core error
    /// gives them.
    parts: Option<Box<Parts>>,
    /// The connection string as given, masked wherever the message quotes it.
    dsn: Option<Box<(Backend, String)>>,
}

#[derive(Debug, Default)]
struct Parts {
    /// The message without the SQL a core refusal quotes, which JSON prints
    /// in its place.
    bare: Option<String>,
    /// JSON's message where it is shorter than text's: what `detail` and
    /// `hint` say is left out of it.
    summary: Option<String>,
    /// The next step to try.
    hint: Option<String>,
    /// The database's own code for the failure, as the driver typed it.
    code: Option<String>,
    /// What the tool that failed said, as it said it.
    detail: Option<String>,
}

pub fn usage(message: impl Into<String>) -> Failure {
    Failure::new(message.into(), true, Category::Usage, Phase::Args)
}

pub fn failed(message: impl Into<String>) -> Failure {
    Failure::new(message.into(), false, Category::Other, Phase::Execute)
}

/// A core error as a failure, with `message` in place of its own text.
/// `message` ends with the error's text, if it quotes it at all.
pub fn caused(message: impl Into<String>, error: &binsql_core::Error) -> Failure {
    use binsql_core::{Error, TransactionOutcome};
    let message = message.into();
    if let Error::Transaction {
        error,
        outcome,
        statement,
    } = error
    {
        let mut failure = caused(message, error);
        failure.statement = *statement;
        failure.transaction = Some(match outcome {
            TransactionOutcome::RolledBack => Transaction::RolledBack,
            TransactionOutcome::Unknown => Transaction::Unknown,
        });
        return failure;
    }
    let (category, phase) = match error {
        Error::UnknownBackend(_) | Error::Placeholders { .. } => (Category::Usage, Phase::Prepare),
        Error::Connect { .. } => (Category::Connect, Phase::Connect),
        Error::Secret { .. } => (Category::Secret, Phase::Connect),
        Error::Query(_) | Error::Transaction { .. } => (Category::Database, Phase::Execute),
        Error::ReadOnly { .. } | Error::NotPlannable { .. } => (Category::Refused, Phase::Prepare),
        Error::Cancelled => (Category::Cancelled, Phase::Execute),
        Error::Config(_) => (Category::Config, Phase::Execute),
        Error::Io(_) => (Category::Io, Phase::Execute),
    };
    // The error's own text, shortened, behind whatever `message` puts
    // before it.
    let shortened = |short: String| {
        message
            .strip_suffix(&error.to_string())
            .map(|prefix| format!("{prefix}{short}"))
    };

    let mut failure = failed(message.clone()).category(category).phase(phase);
    match error {
        Error::ReadOnly { data_source, .. } => {
            failure.parts = Some(Box::new(Parts {
                bare: shortened(format!(
                    "{data_source} is registered read-only; refusing to run the statement"
                )),
                ..Parts::default()
            }));
        }
        Error::NotPlannable { .. } => {
            failure.parts = Some(Box::new(Parts {
                bare: shortened(
                    "only one SELECT, WITH, VALUES or TABLE statement can be planned".to_string(),
                ),
                ..Parts::default()
            }));
        }
        Error::Secret {
            reference,
            reason,
            hint,
            detail,
        } => {
            failure.reason = reason.map(|reason| reason.as_str());
            failure.parts = Some(Box::new(Parts {
                summary: shortened(format!("reading {reference}")),
                hint: hint.clone(),
                detail: Some(detail.clone()),
                ..Parts::default()
            }));
        }
        Error::Connect {
            reason: Some(reason),
            ..
        } => failure.reason = Some(reason.as_str()),
        _ => {}
    }
    if let Some(code) = error.native_code() {
        failure.parts.get_or_insert_default().code = Some(code);
    }
    failure
}

/// A core error as a failure, in its own words.
pub fn core(error: binsql_core::Error) -> Failure {
    caused(error.to_string(), &error)
}

impl Failure {
    fn new(message: String, usage: bool, category: Category, phase: Phase) -> Failure {
        Failure {
            message,
            usage,
            category,
            phase,
            statement: None,
            completed: None,
            transaction: None,
            context: Vec::new(),
            reason: None,
            parts: None,
            dsn: None,
        }
    }

    pub fn category(mut self, category: Category) -> Failure {
        self.category = category;
        self
    }

    pub fn phase(mut self, phase: Phase) -> Failure {
        self.phase = phase;
        self
    }

    pub fn statement(mut self, position: usize) -> Failure {
        self.statement = Some(position);
        self
    }

    pub fn completed(mut self, count: usize) -> Failure {
        self.completed = Some(count);
        self
    }

    pub fn transaction(mut self, transaction: Transaction) -> Failure {
        self.transaction = Some(transaction);
        self
    }

    pub fn context(mut self, line: impl Into<String>) -> Failure {
        self.context.push(line.into());
        self
    }

    fn dsn(mut self, backend: Backend, dsn: String) -> Failure {
        self.dsn = Some(Box::new((backend, dsn)));
        self
    }
}

/// What kind of thing went wrong. Version 1 of the error record; a consumer
/// treats a value it does not know as `other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Usage,
    Source,
    Config,
    Secret,
    Connect,
    Refused,
    Database,
    Cancelled,
    Io,
    Other,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Usage => "usage",
            Category::Source => "source",
            Category::Config => "config",
            Category::Secret => "secret",
            Category::Connect => "connect",
            Category::Refused => "refused",
            Category::Database => "database",
            Category::Cancelled => "cancelled",
            Category::Io => "io",
            Category::Other => "other",
        }
    }
}

/// What became of a batch's transaction when the batch failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transaction {
    /// None was opened: nothing in the batch ran.
    None,
    /// The rollback was confirmed: nothing was kept.
    RolledBack,
    /// The commit or the rollback failed, or the connection was lost.
    Unknown,
}

impl Transaction {
    pub fn as_str(self) -> &'static str {
        match self {
            Transaction::None => "none",
            Transaction::RolledBack => "rolled_back",
            Transaction::Unknown => "unknown",
        }
    }
}

/// How far the command got: up to `prepare` nothing has reached the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Args,
    Input,
    Config,
    Connect,
    Prepare,
    Execute,
    Output,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Args => "args",
            Phase::Input => "input",
            Phase::Config => "config",
            Phase::Connect => "connect",
            Phase::Prepare => "prepare",
            Phase::Execute => "execute",
            Phase::Output => "output",
        }
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

/// Whether failures and notes print as JSON records, settled once per run.
static JSON_ERRORS: OnceLock<bool> = OnceLock::new();

/// Runs a command and returns the process exit code.
pub async fn main(args: Vec<String>) -> i32 {
    let verb = args.first().cloned().unwrap_or_default();
    let rest: Vec<String> = args.into_iter().skip(1).collect();

    // Read before the verb parses its flags, so that a mistake in them comes
    // out in the format asked for too.
    let json = match error_format(&rest) {
        Ok(json) => json,
        Err(failure) => return report(&failure, false),
    };
    JSON_ERRORS.get_or_init(|| json);

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
        Err(failure) => report(&failure, json),
    }
}

/// Whether `--error-format` (the last one before `--`), or else
/// `BINSQL_ERROR_FORMAT`, asks for JSON.
fn error_format(args: &[String]) -> Result<bool> {
    let mut chosen = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if arg == "--" {
            break;
        }
        if arg == "--error-format" {
            let value = args
                .next()
                .ok_or_else(|| usage("--error-format needs a value"))?;
            chosen = Some(value.clone());
        } else if let Some(value) = arg.strip_prefix("--error-format=") {
            chosen = Some(value.to_string());
        }
    }
    let chosen = chosen.or_else(|| {
        std::env::var("BINSQL_ERROR_FORMAT")
            .ok()
            .filter(|value| !value.is_empty())
    });
    match chosen.as_deref() {
        None | Some("text") => Ok(false),
        Some("json") => Ok(true),
        Some(other) => Err(usage(format!(
            "unknown error format {other} — one of: text, json"
        ))),
    }
}

/// Prints the failure and returns the exit code it means.
fn report(failure: &Failure, json: bool) -> i32 {
    let exit = if failure.usage { 2 } else { 1 };
    if json {
        eprintln!("{}", error_record(failure, exit));
        return exit;
    }

    let mut message = failure
        .parts
        .as_ref()
        .and_then(|parts| parts.bare.clone())
        .unwrap_or_else(|| failure.message.clone());
    for line in &failure.context {
        message.push_str("\n  ");
        message.push_str(line);
    }
    eprintln!("error: {message}");
    if failure.usage {
        // A pointer, not the whole help: the message above already says
        // what was wrong, and forty lines under it hide it.
        eprintln!("\nrun `binsql --help` for usage");
    }
    exit
}

/// The version of the error and notice records. Adding a field or a value
/// keeps it; renaming, removing or changing the meaning of one raises it.
const ERROR_SCHEMA: u64 = 1;

/// How much of a tool's own words `detail` keeps.
const DETAIL_LIMIT: usize = 1000;

fn error_record(failure: &Failure, exit: i32) -> String {
    let parts = failure.parts.as_deref();
    let message = parts
        .and_then(|parts| parts.summary.as_ref().or(parts.bare.as_ref()))
        .unwrap_or(&failure.message);
    let mut fields: Vec<(&str, serde_json::Value)> = vec![
        ("type", "error".into()),
        ("schema", ERROR_SCHEMA.into()),
        ("exit", exit.into()),
        ("category", failure.category.as_str().into()),
        ("phase", failure.phase.as_str().into()),
        ("message", failure.cleaned(message).into()),
    ];
    if let Some(reason) = failure.reason {
        fields.push(("reason", reason.into()));
    }
    if let Some(code) = parts.and_then(|parts| parts.code.as_ref()) {
        fields.push(("code", code.as_str().into()));
    }
    if let Some(hint) = parts.and_then(|parts| parts.hint.as_ref()) {
        fields.push(("hint", hint.as_str().into()));
    }
    if let Some(detail) = parts.and_then(|parts| parts.detail.as_ref()) {
        fields.push(("detail", capped(failure.cleaned(detail)).into()));
    }
    if let Some(position) = failure.statement {
        fields.push(("statement", position.into()));
    }
    if let Some(count) = failure.completed {
        fields.push(("completed", count.into()));
    }
    if let Some(transaction) = failure.transaction {
        fields.push(("transaction", transaction.as_str().into()));
    }
    record(&fields)
}

impl Failure {
    /// `text` with the secrets it could quote masked.
    fn cleaned(&self, text: &str) -> String {
        // The stored string is matched verbatim, so it goes before the
        // patterns rewrite any part of it.
        let text = match &self.dsn {
            Some(stored) => {
                let (backend, dsn) = stored.as_ref();
                binsql_core::session::masked(text.to_string(), *backend, dsn)
            }
            None => text.to_string(),
        };
        redact(&text)
    }
}

/// `text` cut to [`DETAIL_LIMIT`] characters, on a character boundary.
fn capped(text: String) -> String {
    match text.char_indices().nth(DETAIL_LIMIT) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text,
    }
}

fn notice_record(message: &str) -> String {
    record(&[
        ("type", "notice".into()),
        ("schema", ERROR_SCHEMA.into()),
        ("message", redact(message).into()),
    ])
}

/// One compact JSON object, its fields in the order given.
fn record(fields: &[(&str, serde_json::Value)]) -> String {
    let fields: Vec<String> = fields
        .iter()
        .map(|(name, value)| format!("{}:{value}", serde_json::Value::from(*name)))
        .collect();
    format!("{{{}}}", fields.join(","))
}

const MASK: &str = "****";

/// `message` with the secrets a connection string or a token can leak masked:
/// the password in a URL's `user:pass@`, the value of `password=`, `pwd=` and
/// `accesstoken=`, and JWT-shaped tokens. What a server says about the data is
/// left as it is.
fn redact(message: &str) -> String {
    let mut message = message.to_string();

    // `scheme://user:pass@host`: the password runs from the first `:` after
    // the scheme to the last `@` before the path.
    let mut from = 0;
    while let Some(found) = message[from..].find("://") {
        let start = from + found + 3;
        let end = message[start..]
            .find(|c: char| c == '/' || c.is_whitespace() || c == '"' || c == '\'')
            .map_or(message.len(), |end| start + end);
        let authority = &message[start..end];
        if let Some(at) = authority.rfind('@')
            && let Some(colon) = authority[..at].find(':')
        {
            message.replace_range(start + colon + 1..start + at, MASK);
        }
        from = start;
    }

    for key in ["password=", "pwd=", "accesstoken="] {
        let mut from = 0;
        while let Some(found) = message[from..].to_ascii_lowercase().find(key) {
            let mut start = from + found + key.len();
            // A quoted or braced value runs to its closing quote or brace,
            // spaces and all.
            let quote = message[start..].chars().next().and_then(|c| match c {
                '\'' | '"' => Some(c),
                '{' => Some('}'),
                _ => None,
            });
            if quote.is_some() {
                start += 1;
            }
            let end = message[start..]
                .find(|c: char| match quote {
                    Some(quote) => c == quote,
                    None => matches!(c, ';' | '&' | '"' | '\'') || c.is_whitespace(),
                })
                .map_or(message.len(), |end| start + end);
            if end > start {
                message.replace_range(start..end, MASK);
                from = start + MASK.len();
            } else {
                from = start;
            }
        }
    }

    let mut from = 0;
    while let Some(found) = message[from..].find("eyJ") {
        let start = from + found;
        let end = message[start..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
            .map_or(message.len(), |end| start + end);
        if message[start..end].matches('.').count() >= 2 {
            message.replace_range(start..end, MASK);
            from = start + MASK.len();
        } else {
            from = end;
        }
    }

    message
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

ERRORS
        --error-format NAME
                          text (default) or json: one JSON line on stderr
                          with a category and a phase, the message redacted;
                          a Key Vault or Azure AD failure adds a reason,
                          a database failure the database's own code

    BINSQL_ERROR_FORMAT says the same through the environment.

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
    "conn",
    "c",
    "dsn",
    "D",
    "driver",
    "d",
    "format",
    "o",
    "catalog",
    "schema",
    "error-format",
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
    let config = Workspace::load()
        .map_err(|error| config_failure(format!("loading connections: {error}")))?;

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
                .ok_or_else(|| no_source(usage(format!("no saved data source named {named}"))))?;
            let source = config
                .get(&id)
                .cloned()
                .ok_or_else(|| no_source(usage(format!("no saved data source named {named}"))))?;
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

    let (backend, dsn) = (source.backend, source.dsn.clone());
    Session::open(name.clone(), source).await.map_err(|error| {
        // A config error out of opening is a secret that would not resolve.
        let category = match error {
            binsql_core::Error::Config(_) | binsql_core::Error::Secret { .. } => Category::Secret,
            _ => Category::Connect,
        };
        caused(format!("connecting to {name}: {error}"), &error)
            .category(category)
            .phase(Phase::Connect)
            .dsn(backend, dsn)
    })
}

/// The data source the config names as its default, and its qualified name.
pub fn default_source(config: &Workspace) -> Result<(String, DataSource)> {
    let named = config.default.clone().ok_or_else(|| {
        no_source(usage(
            "no data source given, and none is the default — pass --conn or --dsn",
        ))
    })?;
    // Through `resolve`, so a `default` of `prod` finds `eimskip/prod`
    // exactly as `--conn prod` does — and says which two it found when
    // that name has stopped meaning one thing.
    let id = config.resolve(&named).ok_or_else(|| {
        no_source(failed(config.unresolved_default().unwrap_or_else(|| {
            format!("the default data source {named} is not in the config")
        })))
    })?;
    let source = config.get(&id).cloned().ok_or_else(|| {
        no_source(failed(format!(
            "the default data source {id} is not in the config"
        )))
    })?;
    Ok((id, source))
}

/// The config could not be read or written.
pub fn config_failure(message: impl Into<String>) -> Failure {
    failed(message)
        .category(Category::Config)
        .phase(Phase::Config)
}

/// No saved data source answers to the name given.
pub fn no_source(failure: Failure) -> Failure {
    failure.category(Category::Source).phase(Phase::Config)
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
            .map_err(|error| input_failure(format!("reading {path}: {error}")));
    }

    let positional = args.positional();
    if positional.len() > 1 {
        return Err(usage(
            "more than one statement was given as an argument — quote the whole script as one",
        )
        .phase(Phase::Input));
    }
    if let Some(sql) = positional.first() {
        if sql == "-" {
            return read_stdin();
        }
        return Ok(sql.clone());
    }

    if std::io::stdin().is_terminal() {
        return Err(
            usage("no SQL given — pass it as an argument, with --file, or on stdin")
                .phase(Phase::Input),
        );
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
        .map_err(|error| input_failure(format!("reading stdin: {error}")))?;
    Ok(buffer)
}

fn input_failure(message: String) -> Failure {
    failed(message).category(Category::Io).phase(Phase::Input)
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
        Err(error) => Err(failed(format!("writing output: {error}"))
            .category(Category::Io)
            .phase(Phase::Output)),
    }
}

/// Says something to the operator without putting it where a parser will find
/// it. Structured output goes to stdout alone.
pub fn note(options: &Options, message: &str) {
    if options.format == Format::None {
        return;
    }
    if JSON_ERRORS.get() == Some(&true) {
        eprintln!("{}", notice_record(message));
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

    fn strings(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_last_error_format_before_the_separator_counts() {
        let json = |list: &[&str]| error_format(&strings(list)).ok();
        assert_eq!(json(&["x", "--error-format", "json"]), Some(true));
        assert_eq!(json(&["--error-format=json", "x"]), Some(true));
        assert_eq!(
            json(&["--error-format=json", "--error-format", "text"]),
            Some(false)
        );
        assert_eq!(
            json(&["--error-format", "text", "--", "--error-format=json"]),
            Some(false)
        );
    }

    #[test]
    fn a_bad_error_format_is_a_usage_error() {
        let failure = error_format(&strings(&["--error-format", "yaml"])).unwrap_err();
        assert!(failure.usage);
        assert_eq!(
            failure.message,
            "unknown error format yaml — one of: text, json"
        );
        let failure = error_format(&strings(&["--error-format"])).unwrap_err();
        assert_eq!(failure.message, "--error-format needs a value");
    }

    #[test]
    fn redaction_masks_each_kind_of_secret() {
        assert_eq!(
            redact("connecting to postgres://u:hunter2@db:5432/x failed"),
            "connecting to postgres://u:****@db:5432/x failed"
        );
        assert_eq!(
            redact("Server=db;User Id=u;Password=hunter2;Pwd=x y"),
            "Server=db;User Id=u;Password=****;Pwd=**** y"
        );
        assert_eq!(
            redact("host=db password='s3 cret' x"),
            "host=db password='****' x"
        );
        assert_eq!(
            redact("Server=db;Password={a;b c};x"),
            "Server=db;Password={****};x"
        );
        assert_eq!(redact("AccessToken=abc&x=1"), "AccessToken=****&x=1");
        assert_eq!(
            redact("bearer eyJhbGciOi.eyJzdWIiOi.c2lnbmF0dXJl rejected"),
            "bearer **** rejected"
        );
        assert_eq!(
            redact("eyJust words. not a token"),
            "eyJust words. not a token"
        );
        assert_eq!(
            redact("relation \"artist\" does not exist"),
            "relation \"artist\" does not exist"
        );
    }

    #[test]
    fn a_record_keeps_its_field_order_and_leaves_the_context_out() {
        let failure = failed("boom")
            .category(Category::Database)
            .statement(2)
            .completed(1)
            .context("statement: delete from t");
        assert_eq!(
            error_record(&failure, 1),
            r#"{"type":"error","schema":1,"exit":1,"category":"database","phase":"execute","message":"boom","statement":2,"completed":1}"#
        );
        assert_eq!(
            notice_record("note: x=\"1\""),
            r#"{"type":"notice","schema":1,"message":"note: x=\"1\""}"#
        );
    }

    #[test]
    fn a_record_masks_the_stored_connection_string() {
        let dsn = "Server=db;Password=hunter2".to_string();
        let failure =
            failed(format!("connecting to x: login failed for {dsn}")).dsn(Backend::MsSql, dsn);
        assert!(!error_record(&failure, 1).contains("hunter2"));
    }

    #[test]
    fn a_record_leaves_out_the_sql_a_core_refusal_quotes() {
        let failure = core(binsql_core::Error::ReadOnly {
            data_source: "ro".to_string(),
            statement: "UPDATE users SET email = 'a@b.c'".to_string(),
        });
        assert!(failure.message.contains("UPDATE users"));
        let record = error_record(&failure, 1);
        assert!(
            !record.contains("UPDATE") && record.contains("refused"),
            "{record}"
        );
    }

    #[test]
    fn a_record_masks_a_stored_password_the_patterns_would_split() {
        let dsn = r#"Server=db;Password="hun""ter2""#.to_string();
        let failure =
            failed(format!("connecting to x: login failed for {dsn}")).dsn(Backend::MsSql, dsn);
        let record = error_record(&failure, 1);
        assert!(
            !record.contains("hun") && !record.contains("ter2"),
            "{record}"
        );
    }

    fn secret(detail: &str) -> binsql_core::Error {
        binsql_core::Error::Secret {
            reference: "keyvault://v/s".to_string(),
            reason: Some(binsql_core::Reason::AzUnauthenticated),
            hint: Some("no usable Azure credential — run `az login`".to_string()),
            detail: detail.to_string(),
        }
    }

    #[test]
    fn a_secret_failure_keeps_its_text_and_splits_its_record() {
        let error = secret("Please run 'az login' to setup account.");
        let failure = caused(format!("connecting to prod: {error}"), &error);
        assert_eq!(failure.message, format!("connecting to prod: {error}"));
        assert_eq!(
            error_record(&failure, 1),
            r#"{"type":"error","schema":1,"exit":1,"category":"secret","phase":"connect","message":"connecting to prod: reading keyvault://v/s","reason":"az-unauthenticated","hint":"no usable Azure credential — run `az login`","detail":"Please run 'az login' to setup account."}"#
        );
    }

    #[test]
    fn a_record_masks_a_token_in_the_detail() {
        let error = secret("AADSTS50173: token eyJhbGciOi.eyJzdWIiOi.c2lnbmF0dXJl expired");
        let record = error_record(&core(error), 1);
        assert!(
            !record.contains("eyJ") && record.contains("token **** expired"),
            "{record}"
        );
    }

    #[test]
    fn a_record_caps_the_detail() {
        let record = error_record(&core(secret(&"é".repeat(DETAIL_LIMIT + 5))), 1);
        let detail = format!("{}…", "é".repeat(DETAIL_LIMIT));
        assert!(
            record.contains(&format!(r#""detail":"{detail}""#)),
            "{record}"
        );
    }

    #[test]
    fn a_token_failure_carries_its_reason() {
        let record = error_record(
            &core(binsql_core::Error::token(anyhow::anyhow!("az exited 1"))),
            1,
        );
        assert!(
            record.contains(r#""category":"connect""#)
                && record.contains(r#""reason":"azure-ad-token""#),
            "{record}"
        );
    }
}
