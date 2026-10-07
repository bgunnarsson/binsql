//! `binsql source` — the saved data sources, read and changed from the command
//! line.
//!
//! It takes its own flags rather than the shared ones: `--conn`, `--catalog`
//! and `--schema` name a database to talk to, and only `test` talks to one,
//! the source it names. It is also the one that resolves a secret: elsewhere
//! a reference prints as written, and a connection string kept in the config
//! prints with its password masked.

use std::time::{Duration, Instant};

use binsql_core::config::mask_dsn;
use binsql_core::secrets::keychain::{self, Keychain, STORE_NAME, SecretStore};
use binsql_core::source::Draft;
use binsql_core::workspace::PROJECT_FILE;
use binsql_core::{
    Backend, Column, DataSource, RESERVED_NAMES, Reference, Resolver, ResultSet, Scope, Session,
    Stage, Value, Workspace,
};

use super::args::Args;
use super::render::Options;
use super::{
    Category, Phase, Result, config_failure, core, default_source, failed, no_source, note, output,
    print, read_stdin, render, usage,
};

const VALUES: &[&str] = &[
    "format",
    "o",
    "scope",
    "dsn",
    "dsn-env",
    "driver",
    "d",
    "description",
    "rename",
    "error-format",
];
const SWITCHES: &[&str] = &[
    "pretty",
    "no-header",
    "no-footer",
    "fresh",
    "dsn-stdin",
    "readonly",
    "no-readonly",
    "open-on-start",
    "no-open-on-start",
    "no-keychain",
    "force",
];

/// The flags only `add` and `edit` take, each with the names it goes by.
const CHANGES: &[&[&str]] = &[
    &["dsn"],
    &["dsn-stdin"],
    &["dsn-env"],
    &["driver", "d"],
    &["description"],
    &["readonly"],
    &["no-readonly"],
    &["open-on-start"],
    &["no-open-on-start"],
    &["no-keychain"],
];

pub async fn run(args: Vec<String>) -> Result<()> {
    let args = Args::parse(args, VALUES, SWITCHES)?;
    let options = output(&args)?;
    let (operation, names) = match args.positional() {
        [] => {
            return Err(usage(
                "source needs a command: list, show, default, test, clear-cache, add, edit or remove",
            ));
        }
        [operation, names @ ..] => (operation.as_str(), names),
    };
    let scope = match args.value(&["scope"]) {
        None => None,
        Some("user") => Some(Scope::User),
        Some("project") => Some(Scope::Project),
        Some(_) => return Err(usage("--scope takes user or project")),
    };
    if scope.is_some() && !matches!(operation, "default" | "add" | "edit") {
        return Err(usage(
            "--scope applies only to source default, add and edit",
        ));
    }
    if args.value(&["rename"]).is_some() && operation != "edit" {
        return Err(usage("--rename applies only to source edit"));
    }
    if !matches!(operation, "add" | "edit")
        && let Some(names) = CHANGES
            .iter()
            .find(|names| args.value(names).is_some() || args.is_set(names))
    {
        return Err(usage(format!(
            "--{} applies only to source add and source edit",
            names[0]
        )));
    }
    let fresh = args.is_set(&["fresh"]);
    if fresh && operation != "test" {
        return Err(usage("--fresh applies only to source test"));
    }
    let force = args.is_set(&["force"]);
    if force && operation != "remove" {
        return Err(usage("--force applies only to source remove"));
    }

    // Why a removed source's secret is still in the store, reported once its
    // row is printed.
    let mut leftover = None;

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
        ("default", []) => {
            let workspace = load()?;
            match workspace.default.as_deref() {
                None => Vec::new(),
                Some(default) => match workspace.resolve(default) {
                    Some(id) => {
                        let source = workspace.get(&id).expect("resolved");
                        vec![row(&workspace, &id, source)]
                    }
                    None => {
                        note(
                            &options,
                            &format!("note: the default {default} is not a saved data source"),
                        );
                        Vec::new()
                    }
                },
            }
        }
        ("default", [name]) => {
            let mut workspace = load()?;
            let id = find(&workspace, name)?;
            workspace.set_default(&id, scope).map_err(core)?;
            let source = workspace.get(&id).expect("resolved");
            vec![row(&workspace, &id, source)]
        }
        ("default", _) => return Err(usage("source default takes at most one name")),
        ("test", [] | [_]) => return test(names.first(), fresh, &options).await,
        ("test", _) => return Err(usage("source test takes at most one name")),
        ("clear-cache", []) => {
            Resolver::from_env().cache().clear().map_err(|error| {
                failed(format!("clearing the secret cache: {error}")).category(Category::Io)
            })?;
            return Ok(());
        }
        ("clear-cache", _) => return Err(usage("source clear-cache takes no name")),
        ("add", [name]) => {
            let mut workspace = load()?;
            let id = add(
                &mut workspace,
                name,
                &Change::read(&args, scope)?,
                &Keychain,
            )?;
            let source = workspace.get(&id).expect("saved");
            vec![row(&workspace, &id, source)]
        }
        ("add", _) => return Err(usage("source add takes one name")),
        ("edit", [name]) => {
            let mut workspace = load()?;
            let id = edit(
                &mut workspace,
                name,
                &Change::read(&args, scope)?,
                &Keychain,
            )?;
            let source = workspace.get(&id).expect("saved");
            vec![row(&workspace, &id, source)]
        }
        ("edit", _) => return Err(usage("source edit takes one name")),
        ("remove", [name]) => {
            if !force {
                return Err(usage(format!(
                    "source remove deletes {name} and its {STORE_NAME} secret; add --force to do it"
                )));
            }
            let mut workspace = load()?;
            let (row, error) = remove(&mut workspace, name, &Keychain)?;
            leftover = error;
            vec![row]
        }
        ("remove", _) => return Err(usage("source remove takes one name")),
        (other, _) => return Err(usage(format!("unknown source command {other}"))),
    };

    let (rows, notes): (Vec<_>, Vec<_>) = rows.into_iter().unzip();
    // No default set is nothing at all, not an empty table or `[]`.
    if !rows.is_empty() || operation != "default" {
        print(&render::rows(&table(rows), &options))?;
    }
    for message in notes.into_iter().flatten() {
        note(&options, &message);
    }
    match leftover {
        Some(message) => Err(failed(message).category(Category::Secret)),
        None => Ok(()),
    }
}

/// Resolves and connects to the named source, or the default, and prints how
/// far it got.
async fn test(name: Option<&String>, fresh: bool, options: &Options) -> Result<()> {
    let workspace = load()?;
    let (id, source) = match name {
        Some(name) => {
            let id = find(&workspace, name)?;
            let source = workspace.get(&id).expect("resolved").clone();
            (id, source)
        }
        None => default_source(&workspace)?,
    };

    let started = Instant::now();
    let outcome = Session::probe(&source, &Resolver::from_env(), fresh).await;
    let elapsed = Value::Int(started.elapsed().as_millis() as i64);

    let row = match &outcome {
        Ok(()) => vec![
            Value::Text(id.clone()),
            Value::Bool(true),
            Value::Null,
            elapsed,
            Value::Null,
        ],
        Err(failure) => vec![
            Value::Text(id.clone()),
            Value::Bool(false),
            Value::Text(failure.stage.as_str().to_string()),
            elapsed,
            Value::Text(failure.message.clone()),
        ],
    };
    print(&render::rows(&test_table(vec![row]), options))?;

    outcome.map_err(|failure| {
        let category = match failure.stage {
            Stage::Secret => Category::Secret,
            Stage::Token | Stage::Connect => Category::Connect,
        };
        failed(format!(
            "{id} failed at the {} stage: {}",
            failure.stage.as_str(),
            failure.message
        ))
        .category(category)
        .phase(Phase::Connect)
    })
}

/// What `add` or `edit` was told, with the connection string already read
/// from wherever it was given.
#[derive(Default)]
struct Change {
    dsn: Option<String>,
    /// Whether the connection string came on `--dsn`, which takes only a
    /// sqlite path or a reference, as anything else would sit in the shell's
    /// history.
    dsn_on_flag: bool,
    backend: Option<Backend>,
    description: Option<String>,
    read_only: Option<bool>,
    open_on_start: Option<bool>,
    scope: Option<Scope>,
    rename: Option<String>,
    no_keychain: bool,
}

impl Change {
    fn read(args: &Args, scope: Option<Scope>) -> Result<Change> {
        let read_only = pair(args, "readonly")?;
        let open_on_start = pair(args, "open-on-start")?;
        let backend = match args.value(&["driver", "d"]) {
            Some(name) => Some(Backend::parse(name).map_err(|error| usage(error.to_string()))?),
            None => None,
        };
        let dsn_on_flag = args.value(&["dsn"]).is_some();
        let given = [
            dsn_on_flag,
            args.is_set(&["dsn-stdin"]),
            args.value(&["dsn-env"]).is_some(),
        ];
        if given.iter().filter(|given| **given).count() > 1 {
            return Err(usage(
                "give the connection string one way: --dsn, --dsn-stdin or --dsn-env",
            ));
        }
        let dsn = if let Some(dsn) = args.value(&["dsn"]) {
            Some(dsn.to_string())
        } else if args.is_set(&["dsn-stdin"]) {
            Some(read_stdin()?.trim_end_matches(['\r', '\n']).to_string())
        } else if let Some(variable) = args.value(&["dsn-env"]) {
            match std::env::var(variable) {
                Ok(dsn) if !dsn.is_empty() => Some(dsn),
                _ => {
                    return Err(usage(format!(
                        "--dsn-env {variable} is not set, or is empty"
                    )));
                }
            }
        } else {
            None
        };
        Ok(Change {
            dsn,
            dsn_on_flag,
            backend,
            description: args.value(&["description"]).map(str::to_string),
            read_only,
            open_on_start,
            scope,
            rename: args.value(&["rename"]).map(str::to_string),
            no_keychain: args.is_set(&["no-keychain"]),
        })
    }
}

/// `--NAME` as true, `--no-NAME` as false, neither as unchanged.
fn pair(args: &Args, name: &str) -> Result<Option<bool>> {
    let negated = format!("no-{name}");
    match (args.is_set(&[name]), args.is_set(&[negated.as_str()])) {
        (true, true) => Err(usage(format!(
            "--{name} and --{negated} cannot both be given"
        ))),
        (true, false) => Ok(Some(true)),
        (false, true) => Ok(Some(false)),
        (false, false) => Ok(None),
    }
}

/// Saves a new data source, and returns the name it was saved under.
fn add(
    workspace: &mut Workspace,
    name: &str,
    change: &Change,
    store: &impl SecretStore,
) -> Result<String> {
    if change.dsn.is_none() {
        return Err(usage(
            "source add needs a connection string: --dsn-stdin, --dsn-env VAR, or --dsn with a sqlite path or a reference",
        ));
    }
    commit(workspace, name, None, change, store)
}

/// Changes a saved data source, and returns the name it is saved under now.
fn edit(
    workspace: &mut Workspace,
    name: &str,
    change: &Change,
    store: &impl SecretStore,
) -> Result<String> {
    let id = find(workspace, name)?;
    let target = change.rename.clone().unwrap_or_else(|| id.clone());
    commit(workspace, &target, Some(&id), change, store)
}

/// Saves `target` as the form would, with what the change leaves out taken
/// from `previous`, the source being edited. Everything refused is refused
/// before anything is written.
fn commit(
    workspace: &mut Workspace,
    target: &str,
    previous: Option<&str>,
    change: &Change,
    store: &impl SecretStore,
) -> Result<String> {
    let stored = previous.and_then(|id| workspace.get(id)).cloned();
    let dsn = change
        .dsn
        .clone()
        .or_else(|| stored.as_ref().map(|source| source.dsn.clone()))
        .unwrap_or_default();
    // An edit that leaves the connection string alone keeps the driver it
    // was saved with rather than guessing again.
    let backend = change
        .backend
        .or_else(|| change.dsn.as_deref().and_then(Backend::infer))
        .or(stored.as_ref().map(|source| source.backend));
    let path = backend == Some(Backend::Sqlite) && is_path(&dsn);
    let filed = keychain::is_reference(&dsn);
    // A path or a reference holds no secret, so it goes where it was given.
    let literal = !filed && !path && !Reference::is_reference(&dsn);

    // One binsql filed under another name would be re-keyed to this one
    // with nothing filed under it.
    if change.dsn.is_some() && filed {
        return Err(usage(
            "a keychain:// reference names a secret binsql filed; pass the connection string with --dsn-stdin or --dsn-env",
        ));
    }
    if change.dsn_on_flag && literal {
        return Err(usage(
            "--dsn takes a sqlite path or a secret reference; pass a connection string with --dsn-stdin or --dsn-env VAR, so it stays out of your shell history",
        ));
    }

    // A literal already in the config stays there unless it is retyped.
    let keychain = if filed {
        !change.no_keychain
    } else {
        literal && change.dsn.is_some() && !change.no_keychain
    };
    let scope = change
        .scope
        .unwrap_or_else(|| workspace.default_scope(previous));
    if scope == Scope::Project && !workspace.has_project() {
        return Err(usage(format!(
            "there is no {PROJECT_FILE} here to save {target} in"
        )));
    }
    let moved = previous.is_some_and(|id| workspace.scope_of(id) != Some(scope));
    if scope == Scope::Project && literal && !keychain && (change.dsn.is_some() || moved) {
        return Err(usage(format!(
            "a connection string is not written into {PROJECT_FILE}: pass it with --dsn-stdin or --dsn-env, without --no-keychain, to file it in the {STORE_NAME}, or keep it in your own config with --scope user"
        )));
    }

    let (folder, name) = target.split_once('/').unwrap_or(("", target));
    let draft = Draft {
        folder,
        name,
        dsn: &dsn,
        backend,
        keychain,
        read_only: change
            .read_only
            .or(stored.as_ref().map(|source| source.read_only))
            .unwrap_or(false),
        open_on_start: change
            .open_on_start
            .or(stored.as_ref().map(|source| source.open_on_start))
            .unwrap_or(false),
        previous: previous.map(str::to_string),
        scope,
    };
    let mut saved = draft.build().map_err(usage)?;
    saved.source.description = change
        .description
        .clone()
        .or(stored.map(|source| source.description))
        .unwrap_or_default();

    let id = saved.id.clone();
    if previous != Some(id.as_str()) && workspace.get(&id).is_some() {
        return Err(usage(match previous {
            None => format!("{id} already exists; change it with binsql source edit {id}"),
            Some(_) => format!("{id} already exists"),
        }));
    }
    // The config would replace the one with the other.
    if let Some((folder, _)) = id.split_once('/')
        && workspace.get(folder).is_some()
    {
        return Err(usage(format!(
            "{folder} is a data source, so it cannot be a folder too"
        )));
    }
    let inside = format!("{id}/");
    if workspace
        .iter()
        .any(|(other, _)| other.starts_with(&inside))
    {
        return Err(usage(format!(
            "{id} is a folder of data sources; save it under another name, or inside it as {id}/NAME"
        )));
    }
    workspace
        .check_new_id(&id)
        .map_err(|error| usage(error.to_string()))?;
    workspace
        .save(saved, store)
        .map_err(|error| failed(format!("saving {id}: {error}")).category(Category::Config))?;
    Ok(id)
}

/// Whether a sqlite connection string is a path, which holds no secret. A
/// keyword string such as `host=h password=p.db` reads as sqlite by its
/// extension, so one with an `=` counts only as a `sqlite:` or `file:` URL.
fn is_path(dsn: &str) -> bool {
    let lower = dsn.trim().to_ascii_lowercase();
    !lower.contains('=') || lower.starts_with("sqlite:") || lower.starts_with("file:")
}

/// Removes the named source from every file that holds it, then its secret.
/// Returns its row as it was, and why its secret is still in the store when
/// deleting it failed: the config is written by then, so that is not undone.
fn remove(
    workspace: &mut Workspace,
    name: &str,
    store: &impl SecretStore,
) -> Result<(Row, Option<String>)> {
    let id = find(workspace, name)?;
    // Built first, since its scope and default are read from the entry.
    let row = row(workspace, &id, workspace.get(&id).expect("resolved"));
    let removed = workspace
        .delete(&id, store)
        .map_err(|error| failed(format!("removing {id}: {error}")).category(Category::Config))?
        .expect("resolved");
    let leftover = removed
        .secret_error
        .map(|error| format!("removed {id}, but its {STORE_NAME} entry is still there: {error}"));
    Ok((row, leftover))
}

fn load() -> Result<Workspace> {
    Workspace::load().map_err(|error| config_failure(format!("loading connections: {error}")))
}

/// The qualified name `name` means, found the way `--conn` finds it.
fn find(workspace: &Workspace, name: &str) -> Result<String> {
    workspace
        .resolve(name)
        .ok_or_else(|| no_source(usage(format!("no saved data source named {name}"))))
}

/// One data source as a row, and the note that goes with it when a command
/// takes its bare name.
type Row = (Vec<Value>, Option<String>);

fn row(workspace: &Workspace, id: &str, source: &DataSource) -> Row {
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

fn test_table(rows: Vec<Vec<Value>>) -> ResultSet {
    ResultSet {
        columns: vec![
            Column::new("name", "text"),
            Column::new("ok", "bool"),
            Column::new("stage", "text"),
            Column::new("elapsed_ms", "int"),
            Column::new("error", "text"),
        ],
        rows,
        rows_affected: None,
        elapsed: Duration::ZERO,
        truncated: false,
    }
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

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::PathBuf;

    use binsql_core::Config;

    use super::*;

    /// A credential store that writes nothing and remembers what it was
    /// asked.
    #[derive(Default)]
    struct Recorder {
        calls: RefCell<Vec<String>>,
        fail: bool,
    }

    impl Recorder {
        fn calls(&self) -> Vec<String> {
            self.calls.borrow().clone()
        }

        fn record(&self, call: String) -> binsql_core::Result<()> {
            self.calls.borrow_mut().push(call);
            if self.fail {
                return Err(std::io::Error::other("the store said no").into());
            }
            Ok(())
        }
    }

    impl SecretStore for Recorder {
        fn set(&self, account: &str, secret: &str) -> binsql_core::Result<()> {
            self.record(format!("set {account} {secret}"))
        }

        fn rename(&self, from: &str, to: &str) -> binsql_core::Result<()> {
            self.record(format!("rename {from} {to}"))
        }

        fn delete(&self, account: &str) -> binsql_core::Result<()> {
            self.record(format!("delete {account}"))
        }
    }

    /// A user file with a keychain source and a sqlite one, and an empty
    /// project file, in a directory of the test's own. Returns the user file.
    fn workspace(test: &str) -> (Workspace, PathBuf) {
        let directory =
            std::env::temp_dir().join(format!("binsql-source-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let user = directory.join("connections.json");
        std::fs::write(
            &user,
            r#"{ "connections": {
                "kc": { "driver": "postgres", "dsn": "keychain://kc" },
                "lite": { "driver": "sqlite", "dsn": "/tmp/l.db" },
                "team": { "a": { "driver": "sqlite", "dsn": "/tmp/a.db" } }
            } }"#,
        )
        .unwrap();
        let project = directory.join(PROJECT_FILE);
        std::fs::write(&project, r#"{ "connections": {} }"#).unwrap();
        let workspace =
            Workspace::load_at(Config::load_from(&user).unwrap(), &user, Some(project)).unwrap();
        (workspace, user)
    }

    #[test]
    fn source_remove_deletes_the_secret_once_the_config_is_written() {
        let (mut workspace, user) = workspace("remove-filed");
        let store = Recorder::default();
        let ((row, _), leftover) = remove(&mut workspace, "kc", &store).unwrap();
        assert_eq!(row[0], Value::Text("kc".into()));
        assert!(leftover.is_none());
        assert_eq!(store.calls(), ["delete kc"]);
        assert!(workspace.get("kc").is_none());
        let config = std::fs::read_to_string(user).unwrap();
        assert!(!config.contains("keychain://kc"), "{config}");

        // A path holds no secret, so the store is not asked.
        remove(&mut workspace, "lite", &store).unwrap();
        assert_eq!(store.calls(), ["delete kc"]);
        assert!(workspace.get("lite").is_none());

        assert!(
            remove(&mut workspace, "nothing", &store)
                .err()
                .unwrap()
                .usage
        );
    }

    #[test]
    fn source_remove_keeps_the_removal_when_the_store_fails() {
        let (mut workspace, user) = workspace("remove-failing");
        let store = Recorder {
            fail: true,
            ..Recorder::default()
        };
        let (_, leftover) = remove(&mut workspace, "kc", &store).unwrap();
        let leftover = leftover.unwrap();
        assert!(leftover.starts_with("removed kc, but its "), "{leftover}");
        assert!(leftover.ends_with("the store said no"), "{leftover}");
        let config = std::fs::read_to_string(user).unwrap();
        assert!(!config.contains("keychain://kc"), "{config}");
    }

    fn typed(dsn: &str) -> Change {
        Change {
            dsn: Some(dsn.into()),
            scope: Some(Scope::User),
            ..Change::default()
        }
    }

    #[test]
    fn source_add_files_a_typed_string_and_saves_its_reference() {
        let (mut workspace, user) = workspace("add-filed");
        let store = Recorder::default();
        let id = add(&mut workspace, "pg", &typed("postgres://u:p@h/db"), &store).unwrap();
        assert_eq!(id, "pg");
        assert_eq!(store.calls(), ["set pg postgres://u:p@h/db"]);
        let config = std::fs::read_to_string(user).unwrap();
        assert!(config.contains("keychain://pg"), "{config}");
        assert!(!config.contains("u:p"), "{config}");

        // A keyword string ending in .db reads as sqlite, but is no path.
        add(&mut workspace, "kw", &typed("host=h password=p.db"), &store).unwrap();
        assert_eq!(store.calls()[1], "set kw host=h password=p.db");
    }

    #[test]
    fn source_add_with_no_keychain_keeps_the_string_in_the_user_file() {
        let (mut workspace, user) = workspace("add-literal");
        let store = Recorder::default();
        let change = Change {
            no_keychain: true,
            ..typed("postgres://u:p@h/db")
        };
        add(&mut workspace, "pg", &change, &store).unwrap();
        assert!(store.calls().is_empty());
        let config = std::fs::read_to_string(user).unwrap();
        assert!(config.contains("postgres://u:p@h/db"), "{config}");
    }

    #[test]
    fn source_edit_rename_carries_the_filed_secret_across() {
        let (mut workspace, _) = workspace("edit-rename");
        let store = Recorder::default();
        let change = Change {
            rename: Some("team/kc".into()),
            ..Change::default()
        };
        let id = edit(&mut workspace, "kc", &change, &store).unwrap();
        assert_eq!(id, "team/kc");
        assert_eq!(store.calls(), ["rename kc team/kc"]);
        assert!(workspace.get("kc").is_none());
        let source = workspace.get("team/kc").unwrap();
        assert_eq!(source.dsn, "keychain://team/kc");
        assert_eq!(source.backend, Backend::Postgres);
    }

    #[test]
    fn source_edit_with_a_new_string_files_it_again() {
        let (mut workspace, _) = workspace("edit-refile");
        let store = Recorder::default();
        let change = Change {
            scope: None,
            ..typed("postgres://h/new")
        };
        edit(&mut workspace, "kc", &change, &store).unwrap();
        assert_eq!(store.calls(), ["set kc postgres://h/new"]);
        assert_eq!(workspace.get("kc").unwrap().dsn, "keychain://kc");
        assert_eq!(workspace.scope_of("kc"), Some(Scope::User));
    }

    #[test]
    fn source_add_with_a_failing_store_writes_no_config() {
        let (mut workspace, user) = workspace("add-failing");
        let before = std::fs::read_to_string(&user).unwrap();
        let store = Recorder {
            fail: true,
            ..Recorder::default()
        };
        let failure = add(&mut workspace, "pg", &typed("postgres://h/db"), &store).unwrap_err();
        assert!(!failure.usage, "{}", failure.message);
        assert_eq!(std::fs::read_to_string(&user).unwrap(), before);
    }

    #[test]
    fn source_refusals_touch_neither_the_store_nor_the_files() {
        let cases: Vec<(&str, Option<&str>, Change)> = vec![
            (
                "pg",
                None,
                Change {
                    dsn_on_flag: true,
                    ..typed("postgres://u:p@h/db")
                },
            ),
            (
                "pg",
                None,
                Change {
                    dsn_on_flag: true,
                    ..typed("keychain://kc")
                },
            ),
            ("kc", None, typed("postgres://h/db")),
            ("source", None, typed("postgres://h/db")),
            (
                "pg",
                None,
                Change {
                    scope: Some(Scope::User),
                    ..Change::default()
                },
            ),
            (
                "pg",
                None,
                Change {
                    scope: Some(Scope::Project),
                    no_keychain: true,
                    ..typed("postgres://h/db")
                },
            ),
            ("pg", None, typed("not a dsn")),
            ("a/b/c", None, typed("postgres://h/db")),
            (
                "pg",
                None,
                Change {
                    backend: Some(Backend::Postgres),
                    ..typed("keychain://kc")
                },
            ),
            ("team", None, typed("./x.db")),
            ("kc/x", None, typed("./x.db")),
            (
                "lite",
                Some("lite"),
                Change {
                    rename: Some("team".into()),
                    ..Change::default()
                },
            ),
            (
                "pg",
                None,
                Change {
                    dsn_on_flag: true,
                    ..typed("host=h password=p.db")
                },
            ),
            (
                "kc",
                Some("lite"),
                Change {
                    rename: Some("lite".into()),
                    ..Change::default()
                },
            ),
            ("nope", Some("nope"), Change::default()),
        ];
        for (index, (name, edited, change)) in cases.into_iter().enumerate() {
            let (mut workspace, user) = workspace(&format!("refused-{index}"));
            let before = std::fs::read_to_string(&user).unwrap();
            let store = Recorder::default();
            let failure = match edited {
                None => add(&mut workspace, name, &change, &store),
                Some(_) => edit(&mut workspace, name, &change, &store),
            }
            .expect_err(name);
            assert!(failure.usage, "{index}: {}", failure.message);
            assert!(store.calls().is_empty(), "{index}");
            assert_eq!(std::fs::read_to_string(&user).unwrap(), before, "{index}");
        }
    }
}
