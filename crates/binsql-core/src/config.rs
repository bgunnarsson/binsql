//! Saved data sources.
//!
//! The on-disk shape is the v2 `connections.json` unchanged, so an existing
//! `~/.config/binsql/connections.json` opens without migration. Everything v3
//! adds is an optional field with a default.

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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default)]
    pub connections: BTreeMap<String, DataSource>,
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
        self.save_to(&Self::path())
    }

    /// Writes owner-only, through a temp file so a failed write cannot truncate
    /// a config that holds credentials.
    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
            set_owner_only(parent, 0o700)?;
        }

        let mut json = serde_json::to_string_pretty(self).map_err(Error::config)?;
        json.push('\n');

        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, json)?;
        set_owner_only(&temp, 0o600)?;
        std::fs::rename(&temp, path)?;
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&DataSource> {
        self.connections.get(name)
    }

    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.connections.keys()
    }

    pub fn set(&mut self, name: impl Into<String>, source: DataSource) {
        self.connections.insert(name.into(), source);
    }

    /// Removes a data source, clearing the default if it pointed there.
    pub fn remove(&mut self, name: &str) -> bool {
        let removed = self.connections.remove(name).is_some();
        if removed && self.default.as_deref() == Some(name) {
            self.default = None;
        }
        removed
    }

    /// The data sources to connect at startup: those flagged `open_on_start`,
    /// or the default one when nothing is flagged.
    pub fn startup_sources(&self) -> Vec<&String> {
        let flagged: Vec<&String> = self
            .connections
            .iter()
            .filter(|(_, source)| source.open_on_start)
            .map(|(name, _)| name)
            .collect();
        if !flagged.is_empty() {
            return flagged;
        }
        self.default
            .as_ref()
            .filter(|name| self.connections.contains_key(*name))
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
}
