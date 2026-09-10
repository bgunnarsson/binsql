//! What the tree found last time, kept on disk.
//!
//! Every level of the explorer costs a round trip, and against a remote SQL
//! Server four of them is the difference between opening a table and waiting to
//! open a table. So each answer is written out under the node that asked for it
//! and read back the next time that node is expanded.
//!
//! The cache is never trusted on its own. A hit is shown immediately *and* the
//! real query still runs; the answer is only applied to the tree when it
//! differs from what was shown. So the common case is instant and silent, a
//! schema that has actually changed redraws itself a moment later, and there is
//! no staleness window to reason about.
//!
//! Nothing here is a secret — object names, not data — so unlike the secret
//! cache this is plain JSON, in the cache directory rather than the config one.
//! Losing all of it costs one slow expansion.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::Serialize;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

use crate::schema::ObjectRef;

/// How long an untouched entry stays readable. Correctness does not depend on
/// this — every hit is revalidated — so it is only here to stop the directory
/// growing forever behind schemas nobody opens any more.
pub const DEFAULT_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// Which set of children an entry holds. The caller reads this back off a key
/// to know what type to ask the cache for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Catalogs,
    Schemas,
    Objects,
    Columns,
}

impl Level {
    fn tag(self) -> &'static str {
        match self {
            Level::Catalogs => "catalogs",
            Level::Schemas => "schemas",
            Level::Objects => "objects",
            Level::Columns => "columns",
        }
    }
}

/// What a cached schema belongs to.
///
/// The name makes the directory recognisable; the connection string is what
/// actually decides whether two of them are the same database. Keying on the
/// name alone would hand a data source repointed at another server the tree it
/// had before — and would have two workspaces that both call something `local`
/// sharing one cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceId {
    name: String,
    dsn: String,
}

impl SourceId {
    pub fn new(name: &str, dsn: &str) -> SourceId {
        SourceId {
            name: name.to_string(),
            dsn: dsn.to_string(),
        }
    }

    /// A directory name that reads as the data source it belongs to —
    /// `eimskip-prod-1f4a…` — with the digest covering the connection string as
    /// well, so the readable half never has to be unique on its own.
    fn slug(&self) -> String {
        let readable: String = self
            .name
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
            .collect();
        format!(
            "{}-{}",
            readable.trim_matches('-'),
            digest(&join([self.name.as_str(), self.dsn.as_str()]))
        )
    }
}

/// What identifies one set of children.
///
/// The source is kept apart from the rest because it names the directory: a
/// data source that goes away takes its whole cache with it in one call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    source: SourceId,
    level: Level,
    path: String,
}

impl Key {
    /// The databases on a connection.
    pub fn catalogs(source: &SourceId) -> Key {
        Key::new(source, Level::Catalogs, [])
    }

    /// The schemas in a database.
    pub fn schemas(source: &SourceId, catalog: &str) -> Key {
        Key::new(source, Level::Schemas, [catalog])
    }

    /// The tables and views in a schema, or in a database on a backend that has
    /// no schemas.
    pub fn objects(source: &SourceId, catalog: &str, schema: Option<&str>) -> Key {
        Key::new(
            source,
            Level::Objects,
            [catalog, schema.unwrap_or_default()],
        )
    }

    /// The columns of one table or view.
    pub fn columns(source: &SourceId, object: &ObjectRef) -> Key {
        Key::new(
            source,
            Level::Columns,
            [
                object.catalog.as_deref().unwrap_or_default(),
                object.schema.as_deref().unwrap_or_default(),
                &object.name,
            ],
        )
    }

    pub fn level(&self) -> Level {
        self.level
    }

    fn new<'a>(source: &SourceId, level: Level, parts: impl IntoIterator<Item = &'a str>) -> Key {
        let mut path = vec![level.tag()];
        path.extend(parts);
        Key {
            source: source.clone(),
            level,
            path: join(path),
        }
    }
}

/// Joins parts with a separator no identifier can contain, so two different
/// paths cannot spell the same key.
fn join<'a>(parts: impl IntoIterator<Item = &'a str>) -> String {
    parts.into_iter().collect::<Vec<_>>().join("\u{1f}")
}

pub struct SchemaCache {
    dir: Option<PathBuf>,
    ttl: Duration,
}

/// One entry as it sits on disk. The key is stored alongside the value so a
/// file found under a hash is checked against the key that produced it rather
/// than trusted for having the right name.
#[derive(Serialize, serde::Deserialize)]
struct Entry<T> {
    source: String,
    path: String,
    at: u64,
    value: T,
}

impl SchemaCache {
    /// The cache under `~/.cache/binsql/schema`, honouring `XDG_CACHE_HOME`.
    /// `BINSQL_SCHEMA_CACHE=0` turns it off, and then every expansion is a
    /// round trip as it was before this existed.
    pub fn from_env() -> SchemaCache {
        let off = std::env::var("BINSQL_SCHEMA_CACHE")
            .is_ok_and(|value| matches!(value.trim(), "0" | "off" | "false"));
        if off {
            return SchemaCache::off();
        }

        let base = match std::env::var("XDG_CACHE_HOME") {
            Ok(dir) if !dir.is_empty() => PathBuf::from(dir),
            _ => dirs::home_dir().unwrap_or_default().join(".cache"),
        };
        SchemaCache::new(base.join("binsql").join("schema"), DEFAULT_TTL)
    }

    pub fn new(dir: impl Into<PathBuf>, ttl: Duration) -> SchemaCache {
        SchemaCache {
            dir: Some(dir.into()),
            ttl,
        }
    }

    /// A cache that holds nothing, for when it is switched off.
    pub fn off() -> SchemaCache {
        SchemaCache {
            dir: None,
            ttl: Duration::ZERO,
        }
    }

    /// What was found here last time, if anything. A file that cannot be read,
    /// cannot be parsed, was written under a different key or has gone stale is
    /// a miss — never an error. The worst a broken cache can do is cost the
    /// round trip it was there to save.
    pub fn get<T: DeserializeOwned>(&self, key: &Key) -> Option<T> {
        let raw = std::fs::read_to_string(self.path(key)?).ok()?;
        let entry: Entry<T> = serde_json::from_str(&raw).ok()?;
        if entry.source != key.source.slug() || entry.path != key.path {
            return None;
        }
        (self.age(entry.at) <= self.ttl).then_some(entry.value)
    }

    /// Writes an entry, or quietly does nothing if it cannot. A cache that
    /// fails to write is not a reason to fail an expansion that worked.
    pub fn put<T: Serialize>(&self, key: &Key, value: &T) {
        let Some(path) = self.path(key) else {
            return;
        };
        let entry = Entry {
            source: key.source.slug(),
            path: key.path.clone(),
            at: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            value,
        };
        let Ok(json) = serde_json::to_string(&entry) else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, json);
    }

    /// Drops one entry, so the next look is a real query. What Refresh means.
    pub fn forget(&self, key: &Key) {
        if let Some(path) = self.path(key) {
            let _ = std::fs::remove_file(path);
        }
    }

    /// Drops everything known about a data source, for when it is removed.
    pub fn forget_source(&self, source: &SourceId) {
        if let Some(dir) = self.dir.as_ref().map(|dir| dir.join(source.slug())) {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    fn path(&self, key: &Key) -> Option<PathBuf> {
        Some(
            self.dir
                .as_ref()?
                .join(key.source.slug())
                .join(format!("{}.json", digest(&key.path))),
        )
    }

    fn age(&self, at: u64) -> Duration {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH + Duration::from_secs(at))
            .unwrap_or_default()
    }

    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }
}

impl Default for SchemaCache {
    fn default() -> Self {
        SchemaCache::from_env()
    }
}

fn digest(value: &str) -> String {
    let hash = Sha256::digest(value.as_bytes());
    hex::encode(&hash[..8])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Catalog, ObjectKind};

    fn cache(test: &str) -> SchemaCache {
        let dir = std::env::temp_dir().join(format!("binsql-schema-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        SchemaCache::new(dir, DEFAULT_TTL)
    }

    fn source(name: &str) -> SourceId {
        SourceId::new(name, "/tmp/one.db")
    }

    fn catalogs() -> Vec<Catalog> {
        vec![Catalog {
            name: "main".into(),
            is_current: true,
        }]
    }

    #[test]
    fn round_trips_a_level() {
        let cache = cache("roundtrip");
        let key = Key::catalogs(&source("eimskip/prod"));

        assert!(cache.get::<Vec<Catalog>>(&key).is_none(), "empty to start");
        cache.put(&key, &catalogs());
        assert_eq!(cache.get::<Vec<Catalog>>(&key), Some(catalogs()));

        cache.forget(&key);
        assert!(cache.get::<Vec<Catalog>>(&key).is_none(), "forgotten");
    }

    #[test]
    fn every_level_gets_its_own_entry() {
        let cache = cache("levels");
        let source = source("s");
        let object = ObjectRef::new(
            Some("app".into()),
            Some("dbo".into()),
            "artist",
            ObjectKind::Table,
        );

        let keys = [
            Key::catalogs(&source),
            Key::schemas(&source, "app"),
            Key::objects(&source, "app", None),
            Key::objects(&source, "app", Some("dbo")),
            Key::columns(&source, &object),
        ];
        for (index, key) in keys.iter().enumerate() {
            cache.put(key, &vec![index]);
        }
        for (index, key) in keys.iter().enumerate() {
            assert_eq!(
                cache.get::<Vec<usize>>(key),
                Some(vec![index]),
                "level {index} collided with another"
            );
        }
    }

    #[test]
    fn two_sources_do_not_share_a_cache() {
        let cache = cache("sources");
        cache.put(&Key::catalogs(&source("eimskip/prod")), &catalogs());

        let other = Key::catalogs(&source("osar/prod"));
        assert!(cache.get::<Vec<Catalog>>(&other).is_none());
        // A name that flattens to the same directory slug is still a different
        // source.
        let flattened = Key::catalogs(&source("eimskip-prod"));
        assert!(cache.get::<Vec<Catalog>>(&flattened).is_none());
    }

    /// The reason the connection string is part of the key: two workspaces can
    /// each hold a `local`, and repointing one at another server must not hand
    /// it the tree the old one had.
    #[test]
    fn the_same_name_pointing_somewhere_else_is_a_different_cache() {
        let cache = cache("repointed");
        let before = SourceId::new("local", "/tmp/before.db");
        let after = SourceId::new("local", "/tmp/after.db");

        cache.put(&Key::catalogs(&before), &catalogs());
        assert!(cache.get::<Vec<Catalog>>(&Key::catalogs(&after)).is_none());
        assert_eq!(
            cache.get::<Vec<Catalog>>(&Key::catalogs(&before)),
            Some(catalogs()),
            "and the original is untouched"
        );
    }

    #[test]
    fn removing_a_source_takes_every_level_with_it() {
        let cache = cache("forget-source");
        let gone = source("gone");
        let keys = [Key::catalogs(&gone), Key::schemas(&gone, "app")];
        for key in &keys {
            cache.put(key, &catalogs());
        }
        let kept = Key::catalogs(&source("stays"));
        cache.put(&kept, &catalogs());

        cache.forget_source(&gone);

        for key in &keys {
            assert!(cache.get::<Vec<Catalog>>(key).is_none(), "{key:?} survived");
        }
        assert_eq!(cache.get::<Vec<Catalog>>(&kept), Some(catalogs()));
    }

    #[test]
    fn a_corrupt_or_stale_entry_is_a_miss_not_an_error() {
        let cache = cache("corrupt");
        let key = Key::catalogs(&source("s"));

        cache.put(&key, &catalogs());
        std::fs::write(cache.path(&key).unwrap(), "{not json").unwrap();
        assert!(cache.get::<Vec<Catalog>>(&key).is_none());

        // An entry older than the TTL, written under a cache that has one.
        let expiring = SchemaCache::new(cache.dir().unwrap(), Duration::ZERO);
        expiring.put(&key, &catalogs());
        std::thread::sleep(Duration::from_millis(1100));
        assert!(expiring.get::<Vec<Catalog>>(&key).is_none(), "stale");
    }

    #[test]
    fn a_cache_that_is_off_holds_nothing() {
        let cache = SchemaCache::off();
        let source = source("s");
        let key = Key::catalogs(&source);
        cache.put(&key, &catalogs());
        assert!(cache.get::<Vec<Catalog>>(&key).is_none());
        // And none of it panics.
        cache.forget(&key);
        cache.forget_source(&source);
    }
}
