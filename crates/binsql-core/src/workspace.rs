//! The data sources in play, which is rarely just one file's worth.
//!
//! Three layers stack into one list:
//!
//! - the **user** config, `~/.config/binsql/connections.json`, which is yours
//!   and every project's;
//! - a **project** config, the nearest `.binsql.json` at or above the working
//!   directory, which is one repository's. Now that a connection string can be
//!   a keychain or Key Vault reference, this file holds names, drivers and
//!   flags and nothing else, so it is meant to be committed;
//! - anything **ephemeral** — a DSN typed on the command line — which is for
//!   this run and is never written anywhere.
//!
//! Later layers win per connection rather than per file, so a project's
//! `eimskip/local` joins your `eimskip/prod` in one `eimskip` folder instead of
//! replacing it.
//!
//! Reads go through the merged view: [`Workspace`] derefs to a [`Config`], so
//! `get`, `iter`, `listing`, `resolve` and `startup_sources` are the same
//! methods they always were. Writes name the layer they mean, because "save
//! this connection" has had two possible answers since the project file
//! existed.

use std::ops::Deref;
use std::path::{Path, PathBuf};

use crate::config::{Config, DataSource, Visibility};
use crate::error::{Error, Result};
use crate::secrets::keychain::{self, SecretStore};
use crate::source::Saved;

/// The file a connection lives in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The nearest `.binsql.json` — this repository's databases.
    Project,
    /// `~/.config/binsql/connections.json` — yours, everywhere.
    User,
}

impl Scope {
    pub fn label(self) -> &'static str {
        match self {
            Scope::Project => "this project",
            Scope::User => "your config",
        }
    }
}

/// The file a project's data sources live in, looked for at and above the
/// working directory.
pub const PROJECT_FILE: &str = ".binsql.json";

/// The bare names `binsql <verb>` takes. A new top-level data source may not
/// use one; inside a folder it may.
pub const RESERVED_NAMES: [&str; 4] = ["query", "exec", "inspect", "source"];

pub struct Workspace {
    user: Config,
    user_path: PathBuf,
    project: Option<Config>,
    project_path: Option<PathBuf>,
    /// For this run only.
    ephemeral: Config,
    /// The three layers flattened, rebuilt on every write. Every read goes
    /// through here.
    merged: Config,
}

impl Deref for Workspace {
    type Target = Config;

    fn deref(&self) -> &Config {
        &self.merged
    }
}

impl Workspace {
    /// Reads the user config and whatever project file the working directory
    /// sits under.
    pub fn load() -> Result<Workspace> {
        let user_path = Config::path();
        Workspace::load_at(Config::load_from(&user_path)?, user_path, project_path())
    }

    /// The same, over paths the caller already knows. The project file is read
    /// if it is there and treated as empty if it is not, so pointing at one
    /// that has yet to be written still lets a connection be saved into it.
    pub fn load_at(
        user: Config,
        user_path: impl Into<PathBuf>,
        project_path: Option<PathBuf>,
    ) -> Result<Workspace> {
        let project = match &project_path {
            Some(path) => Some(Config::load_from(path)?),
            None => None,
        };
        Ok(Workspace::build(
            user,
            user_path.into(),
            project,
            project_path,
        ))
    }

    /// A workspace over one config at a known path, with no project file.
    pub fn single(config: Config, path: impl Into<PathBuf>) -> Workspace {
        Workspace::build(config, path.into(), None, None)
    }

    fn build(
        user: Config,
        user_path: PathBuf,
        project: Option<Config>,
        project_path: Option<PathBuf>,
    ) -> Workspace {
        let mut workspace = Workspace {
            user,
            user_path,
            project,
            project_path,
            ephemeral: Config::default(),
            merged: Config::default(),
        };
        workspace.rebuild();
        workspace
    }

    fn rebuild(&mut self) {
        let mut merged = self.user.clone();
        for layer in [self.project.as_ref(), Some(&self.ephemeral)]
            .into_iter()
            .flatten()
        {
            for (id, source) in layer.iter() {
                merged.set(&id, source.clone());
            }
            if layer.default.is_some() {
                merged.default = layer.default.clone();
            }
        }
        self.merged = merged;
    }

    pub fn user_path(&self) -> &Path {
        &self.user_path
    }

    pub fn project_path(&self) -> Option<&Path> {
        self.project_path.as_deref()
    }

    /// Whether a project file is in play at all. The scope is not a choice
    /// worth offering when there is only one file to save to.
    pub fn has_project(&self) -> bool {
        self.project_path.is_some()
    }

    /// Which file a connection came from, or `None` for one that exists only
    /// for this run.
    pub fn scope_of(&self, id: &str) -> Option<Scope> {
        if self.project.as_ref().is_some_and(|p| p.get(id).is_some()) {
            return Some(Scope::Project);
        }
        self.user.get(id).is_some().then_some(Scope::User)
    }

    /// Where a connection should be saved when nothing says otherwise: back
    /// where it came from, and a new one into the project you are standing in.
    pub fn default_scope(&self, id: Option<&str>) -> Scope {
        match id.and_then(|id| self.scope_of(id)) {
            Some(scope) => scope,
            None if self.has_project() => Scope::Project,
            None => Scope::User,
        }
    }

    /// Refuses a new top-level connection named like a command, which
    /// `binsql <name>` would never open. One already saved stays editable, and
    /// a name inside a folder is never a command.
    pub fn check_new_id(&self, id: &str) -> Result<()> {
        if !id.contains('/') && RESERVED_NAMES.contains(&id) && self.scope_of(id).is_none() {
            return Err(Error::config(anyhow::anyhow!(
                "{id} is a binsql command; save it inside a folder, such as folder/{id}, or under another name"
            )));
        }
        Ok(())
    }

    /// Adds or replaces a connection in one layer and writes that file.
    ///
    /// Any copy in the other layer goes, so moving a connection between files
    /// is a save rather than a save and a delete — otherwise the one left
    /// behind would shadow or be shadowed by the one just written.
    pub fn set(&mut self, id: &str, source: DataSource, scope: Scope) -> Result<()> {
        self.check_new_id(id)?;
        let vacated = match scope {
            Scope::Project => {
                let project = self.project.as_mut().ok_or_else(|| {
                    Error::config(anyhow::anyhow!(
                        "there is no {PROJECT_FILE} here to save {id} in"
                    ))
                })?;
                project.set(id, source);
                self.user.remove(id).then_some(Scope::User)
            }
            Scope::User => {
                self.user.set(id, source);
                self.project
                    .as_mut()
                    .is_some_and(|project| project.remove(id))
                    .then_some(Scope::Project)
            }
        };

        self.write(scope)?;
        if let Some(vacated) = vacated {
            self.write(vacated)?;
        }
        self.rebuild();
        Ok(())
    }

    /// Makes a saved connection the default, in its own file unless `scope`
    /// names another.
    ///
    /// The project file's default wins the merge, so writing the user file
    /// under a project default naming something else is refused rather than
    /// written to no effect. A connection that exists only for this run is
    /// refused too: the next run would find the default naming nothing.
    pub fn set_default(&mut self, id: &str, scope: Option<Scope>) -> Result<()> {
        let Some(own) = self.scope_of(id) else {
            return Err(Error::config(anyhow::anyhow!(
                "{id} is not saved in a file, so it cannot be the default"
            )));
        };
        let scope = scope.unwrap_or(own);
        match scope {
            Scope::User => {
                let project_default = self.project.as_ref().and_then(|p| p.default.clone());
                if let Some(default) = project_default
                    && self.merged.resolve(&default).as_deref() != Some(id)
                {
                    return Err(Error::config(anyhow::anyhow!(
                        "this project's {PROJECT_FILE} sets the default to {default}, which wins over your config; pass --scope project to change it there"
                    )));
                }
                self.user.default = Some(id.to_string());
            }
            Scope::Project => {
                let project = self.project.as_mut().ok_or_else(|| {
                    Error::config(anyhow::anyhow!(
                        "there is no {PROJECT_FILE} here to save the default in"
                    ))
                })?;
                project.default = Some(id.to_string());
            }
        }
        self.write(scope)?;
        self.rebuild();
        Ok(())
    }

    /// Saves what a form or the command line described: files its secret,
    /// writes the config, and on a rename drops the old name.
    ///
    /// The credential store is written before the config, so a config can never
    /// end up naming a secret that was never stored.
    pub fn save(&mut self, saved: Saved, store: &impl SecretStore) -> Result<()> {
        let Saved {
            id,
            source,
            secret,
            previous,
            scope,
        } = saved;
        let renamed_from = previous.filter(|previous| *previous != id);
        // Before the secret, so a refused name files nothing under it.
        self.check_new_id(&id)?;

        match &secret {
            Some(secret) => store.set(&id, secret)?,
            // Nothing new to file, so a rename carries the existing entry
            // across rather than leaving the new name pointing at nothing.
            None if keychain::is_reference(&source.dsn) => {
                if let Some(from) = &renamed_from {
                    store.rename(from, &id)?;
                }
            }
            None => {}
        }

        self.set(&id, source, scope)?;
        if let Some(from) = &renamed_from {
            // Never through `delete`: the old name's secret has either been
            // carried across or superseded by the one just filed, so deleting
            // it would take the live one.
            self.remove(from)?;
        }
        Ok(())
    }

    /// Removes a connection from wherever it is, and writes what changed.
    pub fn remove(&mut self, id: &str) -> Result<bool> {
        let mut removed = false;
        for scope in [Scope::Project, Scope::User] {
            let gone = match scope {
                Scope::Project => self.project.as_mut().is_some_and(|p| p.remove(id)),
                Scope::User => self.user.remove(id),
            };
            if gone {
                self.write(scope)?;
                removed = true;
            }
        }
        if removed {
            self.rebuild();
        }
        Ok(removed)
    }

    /// Removes a connection and then its secret, returning `None` when there
    /// was no such connection.
    pub fn delete(&mut self, id: &str, store: &impl SecretStore) -> Result<Option<Removed>> {
        // Read before the config drops the entry, since it is derived from the
        // connection string it is about to take away.
        let account = self
            .get(id)
            .and_then(|source| keychain::account(&source.dsn))
            .map(str::to_string);
        if !self.remove(id)? {
            return Ok(None);
        }
        // Only after the config is written: a secret left behind by a failed
        // save is recoverable, one deleted for a connection that is still
        // listed is not.
        let secret_error = account.and_then(|account| store.delete(&account).err());
        Ok(Some(Removed { secret_error }))
    }

    /// Registers a connection for this run only — a DSN given on the command
    /// line, which nobody asked to have saved.
    pub fn add_ephemeral(&mut self, id: &str, source: DataSource) {
        self.ephemeral.set(id, source);
        self.rebuild();
    }

    fn write(&self, scope: Scope) -> Result<()> {
        match scope {
            // A project file is committed, so neither it nor the repository it
            // sits in gets its permissions rewritten.
            Scope::Project => match (&self.project, &self.project_path) {
                (Some(project), Some(path)) => project.save_to(path, Visibility::Shared),
                _ => Ok(()),
            },
            Scope::User => self.user.save_to(&self.user_path, Visibility::Private),
        }
    }
}

/// A connection [`Workspace::delete`] took out of the config.
pub struct Removed {
    /// Why its secret is still in the credential store, when deleting it failed.
    pub secret_error: Option<Error>,
}

/// The project file to use: `BINSQL_PROJECT` if it is set, otherwise the
/// nearest `.binsql.json` at or above the working directory.
fn project_path() -> Option<PathBuf> {
    if let Ok(raw) = std::env::var("BINSQL_PROJECT") {
        // An explicit empty value turns discovery off, which is what a test or
        // a CI job wants when the checkout happens to contain one.
        return (!raw.is_empty()).then(|| PathBuf::from(raw));
    }
    let here = std::env::current_dir().ok()?;
    find_project(&here)
}

/// The nearest project file at or above `from`.
pub fn find_project(from: &Path) -> Option<PathBuf> {
    from.ancestors()
        .map(|dir| dir.join(PROJECT_FILE))
        .find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::backend::Backend;

    fn source(dsn: &str) -> DataSource {
        DataSource {
            backend: Backend::Sqlite,
            dsn: dsn.into(),
            description: String::new(),
            read_only: false,
            open_on_start: false,
        }
    }

    /// One directory per test — these run concurrently in the same process, so
    /// a shared path would have them deleting each other's files mid-run.
    fn workspace(test: &str, user: &str, project: Option<&str>) -> Workspace {
        let dir =
            std::env::temp_dir().join(format!("binsql-workspace-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let user_path = dir.join("connections.json");
        let project_path = project.map(|_| dir.join(PROJECT_FILE));
        Workspace::build(
            serde_json::from_str(user).unwrap(),
            user_path,
            project.map(|raw| serde_json::from_str(raw).unwrap()),
            project_path,
        )
    }

    const USER: &str = r#"{
        "connections": {
            "eimskip": { "prod": { "driver": "mssql", "dsn": "keychain://eimskip/prod" } },
            "scratch": { "driver": "sqlite", "dsn": "/tmp/s.db" }
        }
    }"#;

    const PROJECT: &str = r#"{
        "connections": {
            "eimskip": { "local": { "driver": "mssql", "dsn": "keyvault://kv/dsn" } }
        }
    }"#;

    #[test]
    fn a_project_joins_the_user_folder_rather_than_replacing_it() {
        let workspace = workspace("joins", USER, Some(PROJECT));

        assert_eq!(workspace.len(), 3);
        assert!(
            workspace.get("eimskip/prod").is_some(),
            "from the user file"
        );
        assert!(workspace.get("eimskip/local").is_some(), "from the project");
        assert_eq!(
            workspace.listing().len(),
            2,
            "one eimskip folder and one loose connection, not two folders"
        );
    }

    #[test]
    fn the_project_wins_a_collision() {
        let project =
            r#"{ "connections": { "scratch": { "driver": "sqlite", "dsn": "/project.db" } } }"#;
        let workspace = workspace("collision", USER, Some(project));

        assert_eq!(workspace.get("scratch").unwrap().dsn, "/project.db");
        assert_eq!(workspace.scope_of("scratch"), Some(Scope::Project));
    }

    #[test]
    fn scope_says_which_file_a_connection_came_from() {
        let workspace = workspace("scope", USER, Some(PROJECT));
        assert_eq!(workspace.scope_of("eimskip/prod"), Some(Scope::User));
        assert_eq!(workspace.scope_of("eimskip/local"), Some(Scope::Project));
        assert_eq!(workspace.scope_of("nothing"), None);
    }

    #[test]
    fn a_new_connection_defaults_to_the_project_you_are_standing_in() {
        let with = workspace("default-scope", USER, Some(PROJECT));
        assert_eq!(with.default_scope(None), Scope::Project);
        // An existing one goes back where it came from, wherever you are.
        assert_eq!(with.default_scope(Some("eimskip/prod")), Scope::User);

        let without = workspace("default-scope-none", USER, None);
        assert_eq!(without.default_scope(None), Scope::User);
    }

    #[test]
    fn saving_moves_a_connection_between_files_rather_than_copying_it() {
        let mut workspace = workspace("moves", USER, Some(PROJECT));

        workspace
            .set("eimskip/local", source("/moved.db"), Scope::User)
            .expect("saving to the user config");

        assert_eq!(workspace.scope_of("eimskip/local"), Some(Scope::User));
        assert_eq!(workspace.len(), 3, "moved, not duplicated");

        // And back, reading both files off disk this time.
        let reread = Workspace::build(
            Config::load_from(workspace.user_path()).unwrap(),
            workspace.user_path().to_path_buf(),
            Some(Config::load_from(workspace.project_path().unwrap()).unwrap()),
            workspace.project_path().map(Path::to_path_buf),
        );
        assert_eq!(reread.scope_of("eimskip/local"), Some(Scope::User));
        assert_eq!(reread.len(), 3);
    }

    #[test]
    fn removing_takes_it_out_of_whichever_file_had_it() {
        let mut workspace = workspace("removes", USER, Some(PROJECT));

        assert!(workspace.remove("eimskip/local").unwrap());
        assert!(workspace.get("eimskip/local").is_none());
        assert!(!workspace.remove("eimskip/local").unwrap(), "already gone");

        assert!(workspace.remove("eimskip/prod").unwrap());
        assert!(
            workspace.get("eimskip").is_none(),
            "an emptied folder should not linger in either file"
        );
    }

    #[test]
    fn an_ephemeral_source_is_visible_and_never_written() {
        let mut workspace = workspace("ephemeral", USER, Some(PROJECT));
        workspace.add_ephemeral("ad-hoc", source("/tmp/ad-hoc.db"));

        assert!(workspace.get("ad-hoc").is_some());
        assert_eq!(workspace.scope_of("ad-hoc"), None, "it belongs to no file");

        // A later write must not carry it into a file.
        workspace
            .set("scratch", source("/tmp/s2.db"), Scope::User)
            .expect("saving");
        let written = Config::load_from(workspace.user_path()).unwrap();
        assert!(written.get("ad-hoc").is_none());
        assert!(
            workspace.get("ad-hoc").is_some(),
            "still there for this run"
        );
    }

    #[test]
    fn saving_to_a_project_that_does_not_exist_is_refused() {
        let mut workspace = workspace("no-project", USER, None);
        let error = workspace
            .set("new", source("/tmp/n.db"), Scope::Project)
            .expect_err("there is no project file");
        assert!(error.to_string().contains(PROJECT_FILE), "{error}");
    }

    #[test]
    fn the_default_goes_into_the_file_the_source_is_in() {
        let mut workspace = workspace("default-own-file", USER, None);
        workspace.set_default("scratch", None).expect("setting");
        assert_eq!(workspace.default.as_deref(), Some("scratch"));
        let written = Config::load_from(workspace.user_path()).unwrap();
        assert_eq!(written.default.as_deref(), Some("scratch"));
    }

    #[test]
    fn the_default_can_go_into_the_project_file() {
        let mut workspace = workspace("default-project-file", USER, Some(PROJECT));
        workspace
            .set_default("eimskip/local", None)
            .expect("setting");
        let project = Config::load_from(workspace.project_path().unwrap()).unwrap();
        assert_eq!(project.default.as_deref(), Some("eimskip/local"));
        assert!(
            !workspace.user_path().exists(),
            "the user file is left alone"
        );

        workspace
            .set_default("scratch", Some(Scope::Project))
            .expect("setting a user source as the project's default");
        let project = Config::load_from(workspace.project_path().unwrap()).unwrap();
        assert_eq!(project.default.as_deref(), Some("scratch"));
        assert_eq!(workspace.default.as_deref(), Some("scratch"));
    }

    #[test]
    fn a_user_default_under_a_differing_project_default_is_refused() {
        let project = r#"{
            "connections": {
                "eimskip": { "local": { "driver": "mssql", "dsn": "keyvault://kv/dsn" } }
            },
            "default": "eimskip/local"
        }"#;
        let mut workspace = workspace("default-shadowed", USER, Some(project));
        let error = workspace
            .set_default("scratch", None)
            .expect_err("the project's default would win");
        assert!(error.to_string().contains("--scope project"), "{error}");
        assert!(!workspace.user_path().exists(), "nothing written");
        assert_eq!(workspace.default.as_deref(), Some("eimskip/local"));

        workspace
            .set_default("scratch", Some(Scope::Project))
            .expect("the project file can be changed");
        assert_eq!(workspace.default.as_deref(), Some("scratch"));
    }

    #[test]
    fn a_project_default_naming_the_same_source_lets_the_user_file_agree() {
        let project = r#"{
            "connections": {
                "eimskip": { "local": { "driver": "mssql", "dsn": "keyvault://kv/dsn" } }
            },
            "default": "eimskip/local"
        }"#;
        let mut workspace = workspace("default-agrees", USER, Some(project));
        workspace
            .set_default("eimskip/local", Some(Scope::User))
            .expect("a project default naming the same source is no conflict");
        let written = Config::load_from(workspace.user_path()).unwrap();
        assert_eq!(written.default.as_deref(), Some("eimskip/local"));
    }

    #[test]
    fn a_default_needs_a_saved_source_and_a_file_to_go_in() {
        let mut workspace = workspace("default-refused", USER, None);
        let error = workspace
            .set_default("scratch", Some(Scope::Project))
            .expect_err("there is no project file");
        assert!(error.to_string().contains(PROJECT_FILE), "{error}");

        workspace.add_ephemeral("ad-hoc", source("/tmp/ad-hoc.db"));
        assert!(workspace.set_default("ad-hoc", None).is_err());
        assert!(workspace.set_default("ad-hoc", Some(Scope::User)).is_err());
        assert!(workspace.set_default("nope", None).is_err());
        assert!(!workspace.user_path().exists(), "nothing written");
    }

    /// A credential store that writes nothing and remembers what it was
    /// asked, so the save and delete sequences run without the real one.
    #[derive(Default)]
    struct Recorder {
        calls: RefCell<Vec<String>>,
        fail: bool,
        /// The user config, so a delete can note whether the file still
        /// named the secret when it was asked to remove it.
        config: Option<PathBuf>,
    }

    impl Recorder {
        fn failing() -> Recorder {
            Recorder {
                fail: true,
                ..Recorder::default()
            }
        }

        fn calls(&self) -> Vec<String> {
            self.calls.borrow().clone()
        }

        fn record(&self, call: String) -> Result<()> {
            self.calls.borrow_mut().push(call);
            if self.fail {
                return Err(Error::config(anyhow::anyhow!("the store said no")));
            }
            Ok(())
        }
    }

    impl SecretStore for Recorder {
        fn set(&self, account: &str, secret: &str) -> Result<()> {
            self.record(format!("set {account} {secret}"))
        }

        fn rename(&self, from: &str, to: &str) -> Result<()> {
            self.record(format!("rename {from} {to}"))
        }

        fn delete(&self, account: &str) -> Result<()> {
            let listed = self.config.as_ref().is_some_and(|path| {
                std::fs::read_to_string(path)
                    .unwrap_or_default()
                    .contains(&keychain::reference(account))
            });
            self.record(format!("delete {account} listed={listed}"))
        }
    }

    fn saved(id: &str, dsn: &str, secret: Option<&str>, previous: Option<&str>) -> Saved {
        Saved {
            id: id.into(),
            source: source(dsn),
            secret: secret.map(str::to_string),
            previous: previous.map(str::to_string),
            scope: Scope::User,
        }
    }

    #[test]
    fn save_files_a_new_secret_and_lists_the_source() {
        let mut workspace = workspace("save-new", USER, None);
        let store = Recorder::default();
        workspace
            .save(
                saved("acme/prod", "keychain://acme/prod", Some("pg://x"), None),
                &store,
            )
            .unwrap();
        assert_eq!(store.calls(), ["set acme/prod pg://x"]);
        let written = std::fs::read_to_string(workspace.user_path()).unwrap();
        assert!(written.contains("keychain://acme/prod"), "{written}");
    }

    #[test]
    fn save_writes_nothing_when_the_secret_cannot_be_filed() {
        let mut workspace = workspace("save-failing", USER, None);
        let store = Recorder::failing();
        workspace
            .save(
                saved("acme/prod", "keychain://acme/prod", Some("pg://x"), None),
                &store,
            )
            .expect_err("the store refused");
        assert!(workspace.get("acme/prod").is_none());
        assert!(!workspace.user_path().exists());
    }

    #[test]
    fn save_refuses_a_reserved_name_before_the_store() {
        let mut workspace = workspace("save-reserved", USER, None);
        let store = Recorder::default();
        workspace
            .save(
                saved("query", "keychain://query", Some("pg://x"), None),
                &store,
            )
            .expect_err("query is a command");
        assert!(store.calls().is_empty());
    }

    #[test]
    fn save_carries_a_stored_secret_across_a_rename() {
        let mut workspace = workspace("save-rename", USER, None);
        let store = Recorder::default();
        workspace
            .save(
                saved(
                    "eimskip/live",
                    "keychain://eimskip/live",
                    None,
                    Some("eimskip/prod"),
                ),
                &store,
            )
            .unwrap();
        assert_eq!(store.calls(), ["rename eimskip/prod eimskip/live"]);
        assert!(workspace.get("eimskip/live").is_some());
        assert!(workspace.get("eimskip/prod").is_none());
    }

    #[test]
    fn save_files_only_the_new_secret_on_a_retyped_rename() {
        let mut workspace = workspace("save-retyped", USER, None);
        let store = Recorder::default();
        workspace
            .save(
                saved(
                    "eimskip/live",
                    "keychain://eimskip/live",
                    Some("pg://y"),
                    Some("eimskip/prod"),
                ),
                &store,
            )
            .unwrap();
        assert_eq!(store.calls(), ["set eimskip/live pg://y"]);
        assert!(workspace.get("eimskip/prod").is_none());
    }

    #[test]
    fn save_leaves_the_store_alone_for_a_plain_string() {
        let mut workspace = workspace("save-plain", USER, None);
        let store = Recorder::default();
        workspace
            .save(saved("scratch", "/tmp/t.db", None, Some("scratch")), &store)
            .unwrap();
        assert!(store.calls().is_empty());
        assert_eq!(workspace.get("scratch").unwrap().dsn, "/tmp/t.db");
    }

    #[test]
    fn delete_removes_the_secret_only_once_the_config_is_written() {
        let mut workspace = workspace("delete-keychain", USER, None);
        // On disk first, so the recorder has a file that names the secret.
        workspace.write(Scope::User).unwrap();
        let store = Recorder {
            config: Some(workspace.user_path().to_path_buf()),
            ..Recorder::default()
        };
        let removed = workspace.delete("eimskip/prod", &store).unwrap();
        assert!(removed.is_some_and(|removed| removed.secret_error.is_none()));
        assert_eq!(store.calls(), ["delete eimskip/prod listed=false"]);
        assert!(workspace.get("eimskip/prod").is_none());
    }

    #[test]
    fn delete_leaves_the_store_alone_for_a_plain_string() {
        let mut workspace = workspace("delete-plain", USER, None);
        let store = Recorder::default();
        assert!(workspace.delete("scratch", &store).unwrap().is_some());
        assert!(store.calls().is_empty());
    }

    #[test]
    fn delete_of_a_missing_source_does_nothing() {
        let mut workspace = workspace("delete-missing", USER, None);
        let store = Recorder::default();
        assert!(workspace.delete("nowhere", &store).unwrap().is_none());
        assert!(store.calls().is_empty());
    }

    #[test]
    fn delete_reports_a_secret_it_could_not_remove() {
        let mut workspace = workspace("delete-failing", USER, None);
        let store = Recorder::failing();
        let removed = workspace.delete("eimskip/prod", &store).unwrap().unwrap();
        assert!(removed.secret_error.is_some());
        assert!(workspace.get("eimskip/prod").is_none());
    }

    #[test]
    fn a_new_top_level_verb_name_is_refused() {
        let mut workspace = workspace("reserved", USER, None);
        for verb in RESERVED_NAMES {
            let error = workspace
                .set(verb, source("/tmp/q.db"), Scope::User)
                .expect_err("a command");
            assert!(
                error.to_string().contains(&format!("folder/{verb}")),
                "{error}"
            );
            assert!(workspace.get(verb).is_none());
        }
    }

    #[test]
    fn a_verb_name_inside_a_folder_is_saved() {
        let mut workspace = workspace("reserved-folder", USER, None);
        for verb in RESERVED_NAMES {
            let id = format!("folder/{verb}");
            workspace
                .set(&id, source("/tmp/q.db"), Scope::User)
                .expect("a folder entry is never a command");
            assert!(workspace.get(&id).is_some());
        }
    }

    #[test]
    fn a_saved_top_level_verb_name_stays_editable() {
        let user = r#"{ "connections": { "query": { "driver": "sqlite", "dsn": "/tmp/q.db" } } }"#;
        let mut workspace = workspace("reserved-saved", user, Some(PROJECT));
        workspace
            .set("query", source("/tmp/q2.db"), Scope::User)
            .expect("already saved");
        workspace
            .set("query", source("/tmp/q3.db"), Scope::Project)
            .expect("moving it between files");
        assert_eq!(workspace.scope_of("query"), Some(Scope::Project));
    }
}
