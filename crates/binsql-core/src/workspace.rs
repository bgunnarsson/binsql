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

    /// Adds or replaces a connection in one layer and writes that file.
    ///
    /// Any copy in the other layer goes, so moving a connection between files
    /// is a save rather than a save and a delete — otherwise the one left
    /// behind would shadow or be shadowed by the one just written.
    pub fn set(&mut self, id: &str, source: DataSource, scope: Scope) -> Result<()> {
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
}
