//! Saved data sources.
//!
//! A connection can sit at the top level, as v2 wrote them, or inside a named
//! folder — a client, a project — so `eimskip/prod` and `osar/prod` can both
//! exist. Both shapes are read from the same file and both are written back the
//! way they were found, so an existing `connections.json` opens unchanged.
//!
//! A folder is one level deep on purpose. Nesting further would buy a tree that
//! nobody has asked to navigate and a key format that has to escape itself.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::backend::Backend;
use crate::error::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSource {
    #[serde(rename = "driver")]
    pub backend: Backend,
    pub dsn: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Refuses every mutating statement. How a production database should be
    /// registered.
    #[serde(default, rename = "readonly", skip_serializing_if = "is_false")]
    pub read_only: bool,
    /// Connect this data source when binsql starts, rather than on demand.
    #[serde(default, skip_serializing_if = "is_false")]
    pub open_on_start: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// What one key under `connections` holds: either a connection, or a folder of
/// them.
///
/// `untagged` tries `Source` first, so a v2 file — whose values are connection
/// objects — still parses as connections rather than as folders of nothing. A
/// malformed connection fails both arms and is reported, rather than being
/// quietly read as an empty folder.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Entry {
    Source(Box<DataSource>),
    Folder(BTreeMap<String, DataSource>),
}

/// Separates a folder from the connection inside it, in a qualified name like
/// `eimskip/prod`. A connection name may not contain it.
pub const SEPARATOR: char = '/';

/// The qualified name a folder and a connection make together.
pub fn qualify(folder: Option<&str>, name: &str) -> String {
    match folder {
        Some(folder) => format!("{folder}{SEPARATOR}{name}"),
        None => name.to_string(),
    }
}

/// Splits `eimskip/prod` back into its parts. A name with no separator is a
/// top-level connection.
pub fn split_qualified(id: &str) -> (Option<&str>, &str) {
    match id.split_once(SEPARATOR) {
        Some((folder, name)) => (Some(folder), name),
        None => (None, id),
    }
}

/// One row of the sidebar, before anything is connected.
pub enum Listing<'a> {
    Source {
        id: String,
        source: &'a DataSource,
    },
    Folder {
        name: &'a str,
        sources: Vec<(String, &'a DataSource)>,
    },
}

/// Who a config file is for, which decides how it is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    /// Yours alone. Written owner-only, in an owner-only directory: a literal
    /// connection string is one keystroke away even when every DSN in it is a
    /// reference today.
    Private,
    /// Meant to be committed. Rewriting its permissions — or its repository's —
    /// would be wrong, so neither is touched.
    Shared,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default)]
    pub connections: BTreeMap<String, Entry>,
}

impl Config {
    /// The config file location, honouring `BINSQL_CONFIG` and `XDG_CONFIG_HOME`.
    pub fn path() -> PathBuf {
        if let Ok(path) = std::env::var("BINSQL_CONFIG")
            && !path.is_empty()
        {
            return PathBuf::from(path);
        }
        if let Ok(dir) = std::env::var("XDG_CONFIG_HOME")
            && !dir.is_empty()
        {
            return PathBuf::from(dir).join("binsql").join("connections.json");
        }
        dirs::home_dir()
            .unwrap_or_default()
            .join(".config")
            .join("binsql")
            .join("connections.json")
    }

    /// Reads the config, treating a missing file as an empty one.
    pub fn load() -> Result<Config> {
        Self::load_from(&Self::path())
    }

    pub fn load_from(path: &Path) -> Result<Config> {
        let raw = match std::fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
            Err(err) => return Err(Error::Io(err)),
        };
        if raw.trim().is_empty() {
            return Ok(Config::default());
        }
        serde_json::from_str(&raw)
            .map_err(|e| Error::config(anyhow::anyhow!("parsing {}: {e}", path.display())))
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&Self::path(), Visibility::Private)
    }

    /// Writes through a temp file, so a failed write cannot truncate a config
    /// that holds credentials.
    pub fn save_to(&self, path: &Path, visibility: Visibility) -> Result<()> {
        if visibility == Visibility::Private
            && let Some(parent) = path.parent()
        {
            std::fs::create_dir_all(parent)?;
            set_owner_only(parent, 0o700)?;
        }

        let mut json = serde_json::to_string_pretty(self).map_err(Error::config)?;
        json.push('\n');

        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, json)?;
        if visibility == Visibility::Private {
            set_owner_only(&temp, 0o600)?;
        }
        std::fs::rename(&temp, path)?;
        Ok(())
    }

    /// Looks a connection up by its qualified name — `prod`, or
    /// `eimskip/prod`.
    pub fn get(&self, id: &str) -> Option<&DataSource> {
        match split_qualified(id) {
            (None, name) => match self.connections.get(name)? {
                Entry::Source(source) => Some(source),
                Entry::Folder(_) => None,
            },
            (Some(folder), name) => match self.connections.get(folder)? {
                Entry::Folder(sources) => sources.get(name),
                Entry::Source(_) => None,
            },
        }
    }

    /// Every connection, qualified, in the order the sidebar shows them.
    pub fn iter(&self) -> impl Iterator<Item = (String, &DataSource)> {
        self.connections.iter().flat_map(
            |(key, entry)| -> Box<dyn Iterator<Item = (String, &DataSource)>> {
                match entry {
                    Entry::Source(source) => {
                        Box::new(std::iter::once((key.clone(), source.as_ref())))
                    }
                    Entry::Folder(sources) => Box::new(
                        sources
                            .iter()
                            .map(move |(name, source)| (qualify(Some(key), name), source)),
                    ),
                }
            },
        )
    }

    pub fn len(&self) -> usize {
        self.iter().count()
    }

    pub fn is_empty(&self) -> bool {
        self.iter().next().is_none()
    }

    /// The top level, folders intact, for building the sidebar.
    pub fn listing(&self) -> Vec<Listing<'_>> {
        self.connections
            .iter()
            .map(|(key, entry)| match entry {
                Entry::Source(source) => Listing::Source {
                    id: key.clone(),
                    source: source.as_ref(),
                },
                Entry::Folder(sources) => Listing::Folder {
                    name: key,
                    sources: sources
                        .iter()
                        .map(|(name, source)| (qualify(Some(key), name), source))
                        .collect(),
                },
            })
            .collect()
    }

    /// Adds or replaces a connection at a qualified name, creating the folder
    /// if it is new.
    pub fn set(&mut self, id: &str, source: DataSource) {
        match split_qualified(id) {
            (None, name) => {
                self.connections
                    .insert(name.to_string(), Entry::Source(Box::new(source)));
            }
            (Some(folder), name) => {
                let entry = self
                    .connections
                    .entry(folder.to_string())
                    .or_insert_with(|| Entry::Folder(BTreeMap::new()));
                // A folder name that collides with a top-level connection
                // becomes a folder; the connection it replaces was addressed by
                // a name that now means something else.
                if !matches!(entry, Entry::Folder(_)) {
                    *entry = Entry::Folder(BTreeMap::new());
                }
                if let Entry::Folder(sources) = entry {
                    sources.insert(name.to_string(), source);
                }
            }
        }
    }

    /// Removes a connection, dropping the folder if that emptied it and
    /// clearing the default if it pointed there.
    pub fn remove(&mut self, id: &str) -> bool {
        let removed = match split_qualified(id) {
            (None, name) => matches!(self.connections.remove(name), Some(Entry::Source(_))),
            (Some(folder), name) => {
                let Some(Entry::Folder(sources)) = self.connections.get_mut(folder) else {
                    return false;
                };
                let removed = sources.remove(name).is_some();
                if sources.is_empty() {
                    self.connections.remove(folder);
                }
                removed
            }
        };

        if removed && self.default.as_deref() == Some(id) {
            self.default = None;
        }
        removed
    }

    /// Turns what someone typed into a qualified name: an exact match first,
    /// then a connection whose leaf name is unique across every folder, so
    /// `binsql prod` still works when only one folder has one.
    pub fn resolve(&self, needle: &str) -> Option<String> {
        if self.get(needle).is_some() {
            return Some(needle.to_string());
        }
        let mut matches = self.leaf_matches(needle).into_iter();
        let first = matches.next()?;
        matches.next().is_none().then_some(first)
    }

    /// Every qualified name whose leaf is `needle`.
    fn leaf_matches(&self, needle: &str) -> Vec<String> {
        self.iter()
            .map(|(id, _)| id)
            .filter(|id| split_qualified(id).1 == needle)
            .collect()
    }

    /// Why `default` opened nothing, when it names something that does not
    /// resolve to exactly one connection.
    ///
    /// Ambiguity is the case worth a sentence: once two folders each hold a
    /// `prod`, the bare `prod` that was right before names neither, and doing
    /// nothing about it looks identical to having no default at all.
    pub fn unresolved_default(&self) -> Option<String> {
        let name = self.default.as_deref()?;
        if self.resolve(name).is_some() {
            return None;
        }

        let matches = self.leaf_matches(name);
        if matches.is_empty() {
            return Some(format!("default \"{name}\" is not a saved data source"));
        }
        // Short enough to survive the status bar, which is where this is read.
        Some(format!(
            "default \"{name}\" is ambiguous — {}",
            matches.join(" or ")
        ))
    }

    /// The data sources to connect at startup: those flagged `open_on_start`,
    /// or the default one when nothing is flagged.
    pub fn startup_sources(&self) -> Vec<String> {
        let flagged: Vec<String> = self
            .iter()
            .filter(|(_, source)| source.open_on_start)
            .map(|(id, _)| id)
            .collect();
        if !flagged.is_empty() {
            return flagged;
        }
        self.default
            .as_deref()
            .and_then(|name| self.resolve(name))
            .into_iter()
            .collect()
    }
}

#[cfg(unix)]
fn set_owner_only(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_owner_only(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

/// Hides credentials so a DSN can be shown in the UI or a log.
pub fn mask_dsn(dsn: &str) -> String {
    let masked = mask_keyword_password(dsn);

    // URL form: scheme://user:password@host
    if let Some(scheme_end) = masked.find("://") {
        let rest = &masked[scheme_end + 3..];
        if let Some(at) = rest.rfind('@') {
            let credentials = &rest[..at];
            if let Some(colon) = credentials.find(':') {
                return format!(
                    "{}{}:****{}",
                    &masked[..scheme_end + 3],
                    &credentials[..colon],
                    &rest[at..]
                );
            }
        }
        return masked;
    }

    // go-sql-driver form: user:password@tcp(...)
    if let Some(at) = masked.rfind('@') {
        let credentials = &masked[..at];
        if let Some(colon) = credentials.find(':')
            && !credentials.contains(' ')
        {
            return format!("{}:****{}", &credentials[..colon], &masked[at..]);
        }
    }

    masked
}

/// Replaces the value of any `password=` / `pwd=` term, in either the
/// semicolon-separated or space-separated keyword form.
fn mask_keyword_password(dsn: &str) -> String {
    let mut out = String::with_capacity(dsn.len());
    let mut rest = dsn;

    while let Some(position) = find_password_key(rest) {
        let (key_start, value_start) = position;
        out.push_str(&rest[..value_start]);
        out.push_str("****");
        let value_end = rest[value_start..]
            .find([';', ' '])
            .map(|offset| value_start + offset)
            .unwrap_or(rest.len());
        rest = &rest[value_end..];
        let _ = key_start;
    }

    out.push_str(rest);
    out
}

/// Finds the next `password=` / `pwd=` term, returning where the key and its
/// value begin.
fn find_password_key(haystack: &str) -> Option<(usize, usize)> {
    let lower = haystack.to_ascii_lowercase();
    let mut search_from = 0;

    while search_from < lower.len() {
        let candidates = ["password", "pwd"];
        let next = candidates
            .iter()
            .filter_map(|key| {
                lower[search_from..]
                    .find(key)
                    .map(|at| (search_from + at, *key))
            })
            .min_by_key(|(at, _)| *at)?;
        let (key_start, key) = next;

        // Only a term boundary counts, so `old_password` is not mistaken for a
        // separate key and `passwordless=1` is not truncated.
        let preceded_by_boundary =
            key_start == 0 || matches!(lower.as_bytes()[key_start - 1], b';' | b' ' | b'&' | b'?');
        let after_key = key_start + key.len();
        let value_start = lower[after_key..]
            .find('=')
            .filter(|offset| lower[after_key..after_key + offset].trim().is_empty())
            .map(|offset| after_key + offset + 1);

        if preceded_by_boundary && let Some(value_start) = value_start {
            return Some((key_start, value_start));
        }
        search_from = after_key;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_url_credentials() {
        assert_eq!(
            mask_dsn("postgres://app:hunter2@db.example.com:5432/app"),
            "postgres://app:****@db.example.com:5432/app"
        );
    }

    #[test]
    fn masks_keyword_password() {
        assert_eq!(
            mask_dsn("server=tcp:h,1433;user id=sa;password=hunter2;database=app"),
            "server=tcp:h,1433;user id=sa;password=****;database=app"
        );
    }

    #[test]
    fn leaves_password_free_dsn_alone() {
        assert_eq!(mask_dsn("/data/app.db"), "/data/app.db");
        assert_eq!(
            mask_dsn("host=localhost dbname=app sslmode=disable"),
            "host=localhost dbname=app sslmode=disable"
        );
    }

    #[test]
    fn reads_a_v2_config_unchanged() {
        let raw = r#"{
            "default": "local",
            "connections": {
                "local": {
                    "driver": "postgres",
                    "dsn": "postgres://localhost/app",
                    "description": "dev",
                    "readonly": true
                }
            }
        }"#;
        let config: Config = serde_json::from_str(raw).expect("v2 config parses");
        let source = config.get("local").expect("connection present");
        assert_eq!(source.backend, Backend::Postgres);
        assert!(source.read_only);
        assert!(!source.open_on_start);
        assert_eq!(config.startup_sources(), vec!["local"]);
    }

    const FOLDERED: &str = r#"{
        "default": "eimskip/prod",
        "connections": {
            "eimskip": {
                "prod": { "driver": "mssql", "dsn": "keyvault://kv-eimskip-prd/dsn", "readonly": true },
                "local": { "driver": "mssql", "dsn": "keyvault://kv-eimskip-local/dsn" }
            },
            "osar": {
                "prod": { "driver": "mssql", "dsn": "keyvault://kv-osar-prd/dsn", "readonly": true }
            },
            "scratch": { "driver": "sqlite", "dsn": "/tmp/scratch.db" }
        }
    }"#;

    #[test]
    fn reads_folders_and_loose_connections_together() {
        let config: Config = serde_json::from_str(FOLDERED).expect("parses");

        assert_eq!(config.len(), 4);
        assert_eq!(
            config.get("eimskip/prod").expect("in a folder").dsn,
            "keyvault://kv-eimskip-prd/dsn"
        );
        assert_eq!(
            config.get("scratch").expect("top level").backend,
            Backend::Sqlite
        );

        // The same leaf name in two folders is two different connections.
        assert_ne!(
            config.get("eimskip/prod").unwrap().dsn,
            config.get("osar/prod").unwrap().dsn
        );

        // A folder is not a connection, and a connection is not a folder.
        assert!(config.get("eimskip").is_none());
        assert!(config.get("scratch/prod").is_none());
    }

    #[test]
    fn resolves_a_bare_name_only_when_it_is_unambiguous() {
        let config: Config = serde_json::from_str(FOLDERED).expect("parses");

        assert_eq!(
            config.resolve("eimskip/prod").as_deref(),
            Some("eimskip/prod")
        );
        assert_eq!(config.resolve("scratch").as_deref(), Some("scratch"));
        // `local` exists in one folder only.
        assert_eq!(config.resolve("local").as_deref(), Some("eimskip/local"));
        // `prod` exists in two, so it names nothing on its own.
        assert_eq!(config.resolve("prod"), None);
        assert_eq!(config.resolve("nothing"), None);
    }

    #[test]
    fn the_default_may_be_qualified() {
        let config: Config = serde_json::from_str(FOLDERED).expect("parses");
        assert_eq!(config.startup_sources(), vec!["eimskip/prod"]);
    }

    #[test]
    fn a_folder_round_trips_through_the_file() {
        let config: Config = serde_json::from_str(FOLDERED).expect("parses");
        let written = serde_json::to_string(&config).expect("serialises");
        let reread: Config = serde_json::from_str(&written).expect("re-parses");

        assert_eq!(reread.len(), 4);
        assert_eq!(
            reread.get("osar/prod").expect("still in its folder").dsn,
            "keyvault://kv-osar-prd/dsn"
        );
        assert!(reread.get("scratch").is_some(), "still top level");
    }

    #[test]
    fn set_creates_a_folder_and_remove_drops_an_empty_one() {
        let mut config = Config::default();
        config.set(
            "osar/prod",
            DataSource {
                backend: Backend::MsSql,
                dsn: "keyvault://kv/dsn".into(),
                description: String::new(),
                read_only: true,
                open_on_start: false,
            },
        );
        assert!(config.get("osar/prod").is_some());
        assert!(matches!(
            config.connections.get("osar"),
            Some(Entry::Folder(_))
        ));

        assert!(config.remove("osar/prod"));
        assert!(
            !config.connections.contains_key("osar"),
            "an emptied folder should not linger"
        );
    }

    #[test]
    fn listing_keeps_folders_whole() {
        let config: Config = serde_json::from_str(FOLDERED).expect("parses");
        let listing = config.listing();
        assert_eq!(listing.len(), 3, "two folders and one loose connection");

        match &listing[0] {
            Listing::Folder { name, sources } => {
                assert_eq!(*name, "eimskip");
                assert_eq!(sources.len(), 2);
                assert_eq!(sources[0].0, "eimskip/local");
            }
            Listing::Source { .. } => panic!("eimskip is a folder"),
        }
        assert!(matches!(
            &listing[2],
            Listing::Source { id, .. } if id == "scratch"
        ));
    }

    /// The shape as it was actually handed over, verbatim, including the
    /// `default` that no longer names one thing.
    #[test]
    fn the_folder_shape_as_written_by_hand() {
        let raw = r#"{
  "default": "prod",
  "connections": {
    "eimskip":{
      "prod": {
        "driver": "mssql",
        "dsn": "keyvault://kv-eimskip-prd/ConnectionStrings--umbracoDbDSN",
        "readonly": true
      },
      "local": {
        "driver": "mssql",
        "dsn": "keyvault://kv-eimskip-local/ConnectionStrings--umbracoDbDSN",
        "readonly": true
      }
    },
    "osar":{
      "prod": {
        "driver": "mssql",
        "dsn": "keyvault://kv-osar-prd/ConnectionStrings--umbracoDbDSN",
        "readonly": true
      },
      "local": {
        "driver": "mssql",
        "dsn": "keyvault://kv-osar-local/ConnectionStrings--umbracoDbDSN",
        "readonly": true
      }
    }
  }
}"#;
        let config: Config = serde_json::from_str(raw).expect("parses");
        assert_eq!(config.len(), 4);
        assert_eq!(config.listing().len(), 2, "two folders");
        assert!(config.get("osar/local").unwrap().read_only);

        // "prod" now names two connections, so it names neither. Nothing opens
        // on start rather than something arbitrary opening — but silence would
        // be indistinguishable from having no default, so it says which two.
        assert_eq!(config.resolve("prod"), None);
        assert!(config.startup_sources().is_empty());

        let problem = config.unresolved_default().expect("the default is broken");
        assert!(problem.contains("eimskip/prod"), "{problem}");
        assert!(problem.contains("osar/prod"), "{problem}");
    }

    #[test]
    fn a_default_that_resolves_has_nothing_to_report() {
        let config: Config = serde_json::from_str(FOLDERED).expect("parses");
        assert_eq!(config.unresolved_default(), None);

        let none: Config = serde_json::from_str(r#"{ "connections": {} }"#).expect("parses");
        assert_eq!(
            none.unresolved_default(),
            None,
            "no default is not a problem"
        );
    }

    #[test]
    fn a_default_naming_nothing_says_so() {
        let raw = r#"{ "default": "gone", "connections": { "scratch": { "driver": "sqlite", "dsn": "/tmp/s.db" } } }"#;
        let config: Config = serde_json::from_str(raw).expect("parses");
        let problem = config.unresolved_default().expect("the default is broken");
        assert!(problem.contains("is not a saved data source"), "{problem}");
    }

    #[test]
    fn a_malformed_connection_is_reported_not_read_as_a_folder() {
        let raw = r#"{ "connections": { "broken": { "driver": "postgres" } } }"#;
        assert!(serde_json::from_str::<Config>(raw).is_err());
    }
}
