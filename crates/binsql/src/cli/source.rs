//! `binsql source` — the saved data sources, read and changed from the command
//! line.
//!
//! It takes its own flags rather than the shared ones: `--conn`, `--catalog`
//! and `--schema` name a database to talk to, and nothing here talks to one.
//! Nor does it resolve a secret: a reference prints as written, and a
//! connection string kept in the config prints with its password masked.

use std::time::Duration;

use binsql_core::config::mask_dsn;
use binsql_core::secrets::keychain;
use binsql_core::{
    Column, DataSource, RESERVED_NAMES, Reference, ResultSet, Scope, Value, Workspace,
};

use super::args::Args;
use super::{Result, failed, note, output, print, render, usage};

const VALUES: &[&str] = &["format", "o"];
const SWITCHES: &[&str] = &["pretty", "no-header", "no-footer"];

pub async fn run(args: Vec<String>) -> Result<()> {
    let args = Args::parse(args, VALUES, SWITCHES)?;
    let options = output(&args)?;
    let (operation, names) = match args.positional() {
        [] => return Err(usage("source needs a command: list or show")),
        [operation, names @ ..] => (operation.as_str(), names),
    };

    let rows = match (operation, names) {
        ("list", []) => {
            let workspace = load()?;
            workspace
                .iter()
                .map(|(id, source)| row(&workspace, &id, source))
                .collect()
        }
        ("list", _) => return Err(usage("source list takes no name")),
        ("show", [name]) => {
            let workspace = load()?;
            let id = find(&workspace, name)?;
            let source = workspace.get(&id).expect("resolved");
            vec![row(&workspace, &id, source)]
        }
        ("show", _) => return Err(usage("source show takes one name")),
        (other, _) => return Err(usage(format!("unknown source command {other}"))),
    };

    let (rows, notes): (Vec<_>, Vec<_>) = rows.into_iter().unzip();
    print(&render::rows(&table(rows), &options))?;
    for message in notes.into_iter().flatten() {
        note(&options, &message);
    }
    Ok(())
}

fn load() -> Result<Workspace> {
    Workspace::load().map_err(|error| failed(format!("loading connections: {error}")))
}

/// The qualified name `name` means, found the way `--conn` finds it.
fn find(workspace: &Workspace, name: &str) -> Result<String> {
    workspace
        .resolve(name)
        .ok_or_else(|| usage(format!("no saved data source named {name}")))
}

/// One data source as a row, and the note that goes with it when a command
/// takes its bare name.
fn row(workspace: &Workspace, id: &str, source: &DataSource) -> (Vec<Value>, Option<String>) {
    let scope = match workspace.scope_of(id) {
        Some(Scope::User) => Value::Text("user".into()),
        Some(Scope::Project) => Value::Text("project".into()),
        None => Value::Null,
    };
    let dsn = if keychain::is_reference(&source.dsn) || Reference::is_reference(&source.dsn) {
        source.dsn.clone()
    } else {
        mask_dsn(source.backend, &source.dsn)
    };
    let default = workspace
        .default
        .as_deref()
        .and_then(|name| workspace.resolve(name))
        .is_some_and(|default| default == id);

    // Shadowed when `binsql <leaf>` would have opened it but a command takes
    // the name: a top-level entry, or a folder's that is the only one so named.
    let leaf = leaf(id);
    let shadowed = RESERVED_NAMES.contains(&leaf) && workspace.resolve(leaf).as_deref() == Some(id);
    let message = shadowed.then(|| {
        if leaf == "source" {
            format!("note: binsql source alone still opens {id}")
        } else {
            format!(
                "note: binsql {leaf} runs the command; open it with binsql -- {leaf}, binsql {id} or --conn {id}"
            )
        }
    });

    let values = vec![
        Value::Text(id.to_string()),
        scope,
        Value::Text(source.backend.as_str().to_string()),
        Value::Text(dsn),
        Value::Bool(source.read_only),
        Value::Bool(source.open_on_start),
        Value::Bool(default),
        Value::Text(source.description.clone()),
        Value::Bool(shadowed),
    ];
    (values, message)
}

fn leaf(id: &str) -> &str {
    id.rsplit('/').next().unwrap_or(id)
}

fn table(rows: Vec<Vec<Value>>) -> ResultSet {
    ResultSet {
        columns: vec![
            Column::new("name", "text"),
            Column::new("scope", "text"),
            Column::new("driver", "text"),
            Column::new("dsn", "text"),
            Column::new("readonly", "bool"),
            Column::new("open_on_start", "bool"),
            Column::new("default", "bool"),
            Column::new("description", "text"),
            Column::new("shadowed", "bool"),
        ],
        rows,
        rows_affected: None,
        elapsed: Duration::ZERO,
        truncated: false,
    }
}
