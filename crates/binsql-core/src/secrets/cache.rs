//! An on-disk cache of resolved secrets, in v2's format.
//!
//! Entries are encrypted with AES-256-GCM under a key kept beside the cache,
//! and both files are written `0600`. Be clear about the threat model: because
//! the key sits next to the ciphertext, this protects against a secret being
//! scooped up incidentally — by a backup, a directory sync, a shared screen, or
//! a grep across the home directory — not against someone who can already read
//! your files as you. It is a meaningful reduction in accidental exposure, not
//! a vault of its own. A TTL of zero keeps secrets off the disk entirely.
//!
//! The file format, the key file, the hashed entry keys and the associated data
//! all match binsql 2 exactly, so the two versions share one cache while both
//! are installed. Changing any of them silently invalidates the other's
//! entries, which is why `reference::Reference::canonical` is pinned by a test.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::reference::Reference;
use crate::error::{Error, Result};

/// How long a resolved secret stays cached, matching v2.
pub const DEFAULT_TTL: Duration = Duration::from_secs(15 * 60);

const CACHE_VERSION: u32 = 1;
const KEY_BYTES: usize = 32;
const NONCE_BYTES: usize = 12;

#[derive(Debug, Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    entries: BTreeMap<String, CacheEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    nonce: String,
    value: String,
    expires_at: DateTime<Utc>,
}

pub struct Cache {
    dir: PathBuf,
    ttl: Duration,
}

impl Cache {
    pub fn new(dir: impl Into<PathBuf>, ttl: Duration) -> Cache {
        Cache {
            dir: dir.into(),
            ttl,
        }
    }

    fn enabled(&self) -> bool {
        !self.ttl.is_zero() && !self.dir.as_os_str().is_empty()
    }

    fn cache_path(&self) -> PathBuf {
        self.dir.join("secret-cache.json")
    }

    fn key_path(&self) -> PathBuf {
        self.dir.join("cache.key")
    }

    /// Returns a cached secret, or `None` when absent, expired or unreadable.
    /// A damaged cache is never fatal — the caller simply refetches.
    pub fn get(&self, reference: &Reference) -> Option<String> {
        if !self.enabled() {
            return None;
        }
        let file = self.load().ok()?;
        let entry = file.entries.get(&entry_key(reference))?;
        if Utc::now() > entry.expires_at {
            return None;
        }
        self.decrypt(reference, entry).ok()
    }

    /// Stores a secret until the TTL elapses.
    pub fn put(&self, reference: &Reference, value: &str) -> Result<()> {
        if !self.enabled() {
            return Ok(());
        }
        std::fs::create_dir_all(&self.dir)?;
        set_owner_only(&self.dir, 0o700)?;

        let entry = self.encrypt(reference, value)?;

        let mut file = self.load().unwrap_or_else(|_| CacheFile {
            version: CACHE_VERSION,
            entries: BTreeMap::new(),
        });
        file.entries.insert(entry_key(reference), entry);

        // Drop expired entries so the file does not grow without bound.
        let now = Utc::now();
        file.entries.retain(|_, entry| entry.expires_at > now);

        self.save(&file)
    }

    /// Removes every cached secret and the key that protected them.
    pub fn clear(&self) -> Result<()> {
        for path in [self.cache_path(), self.key_path()] {
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(Error::Io(err)),
            }
        }
        Ok(())
    }

    /// How many unexpired entries are cached.
    pub fn count(&self) -> usize {
        let Ok(file) = self.load() else {
            return 0;
        };
        let now = Utc::now();
        file.entries
            .values()
            .filter(|entry| entry.expires_at > now)
            .count()
    }

    fn load(&self) -> Result<CacheFile> {
        let raw = std::fs::read_to_string(self.cache_path())?;
        let file: CacheFile = serde_json::from_str(&raw).map_err(Error::config)?;
        if file.version != CACHE_VERSION {
            return Err(Error::config(anyhow::anyhow!(
                "unsupported cache version {}",
                file.version
            )));
        }
        Ok(file)
    }

    fn save(&self, file: &CacheFile) -> Result<()> {
        let mut json = serde_json::to_string_pretty(file).map_err(Error::config)?;
        json.push('\n');

        let temp = self.cache_path().with_extension("json.tmp");
        std::fs::write(&temp, json)?;
        set_owner_only(&temp, 0o600)?;
        std::fs::rename(&temp, self.cache_path())?;
        Ok(())
    }

    /// Loads the local key, generating one on first use. A corrupt key file
    /// means the existing entries are unreadable anyway, so it is replaced
    /// rather than reported — the effect is a refetch.
    fn key(&self) -> Result<[u8; KEY_BYTES]> {
        if let Ok(encoded) = std::fs::read_to_string(self.key_path())
            && let Ok(decoded) = BASE64.decode(encoded.trim())
            && let Ok(key) = <[u8; KEY_BYTES]>::try_from(decoded.as_slice())
        {
            return Ok(key);
        }

        let mut key = [0u8; KEY_BYTES];
        getrandom::fill(&mut key)
            .map_err(|e| Error::config(anyhow::anyhow!("generating a cache key: {e}")))?;

        std::fs::create_dir_all(&self.dir)?;
        set_owner_only(&self.dir, 0o700)?;
        std::fs::write(self.key_path(), BASE64.encode(key))?;
        set_owner_only(&self.key_path(), 0o600)?;
        Ok(key)
    }

    fn cipher(&self) -> Result<Aes256Gcm> {
        let key = self.key()?;
        Aes256Gcm::new_from_slice(&key)
            .map_err(|e| Error::config(anyhow::anyhow!("preparing the cache cipher: {e}")))
    }

    fn encrypt(&self, reference: &Reference, value: &str) -> Result<CacheEntry> {
        let cipher = self.cipher()?;

        let mut nonce = [0u8; NONCE_BYTES];
        getrandom::fill(&mut nonce)
            .map_err(|e| Error::config(anyhow::anyhow!("generating a nonce: {e}")))?;

        // Bind the ciphertext to its reference so an entry cannot be swapped
        // for another vault's secret by editing the file.
        let associated = reference.canonical();
        let sealed = cipher
            .encrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: value.as_bytes(),
                    aad: associated.as_bytes(),
                },
            )
            .map_err(|e| Error::config(anyhow::anyhow!("encrypting the cache entry: {e}")))?;

        let expires_at = Utc::now()
            + chrono::Duration::from_std(self.ttl)
                .unwrap_or_else(|_| chrono::Duration::minutes(15));

        Ok(CacheEntry {
            nonce: BASE64.encode(nonce),
            value: BASE64.encode(sealed),
            expires_at,
        })
    }

    fn decrypt(&self, reference: &Reference, entry: &CacheEntry) -> Result<String> {
        let cipher = self.cipher()?;
        let nonce = BASE64.decode(&entry.nonce).map_err(Error::config)?;
        let sealed = BASE64.decode(&entry.value).map_err(Error::config)?;
        let associated = reference.canonical();
        let nonce = <[u8; NONCE_BYTES]>::try_from(nonce.as_slice())
            .map_err(|_| Error::config(anyhow::anyhow!("bad nonce length")))?;
        let plain = cipher
            .decrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: &sealed,
                    aad: associated.as_bytes(),
                },
            )
            .map_err(|e| Error::config(anyhow::anyhow!("decrypting the cache entry: {e}")))?;

        String::from_utf8(plain).map_err(Error::config)
    }
}

/// Hashes the reference, so the file does not disclose which vaults and secret
/// names are in use.
fn entry_key(reference: &Reference) -> String {
    let digest = Sha256::digest(reference.canonical().as_bytes());
    hex::encode(digest)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "binsql-cache-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn reference() -> Reference {
        Reference::parse("keyvault://kv/dsn", None).unwrap()
    }

    #[test]
    fn round_trips_a_secret() {
        let dir = temp_dir("roundtrip");
        let cache = Cache::new(&dir, DEFAULT_TTL);

        assert_eq!(cache.get(&reference()), None);
        cache
            .put(&reference(), "Server=db;Password=hunter2")
            .unwrap();
        assert_eq!(
            cache.get(&reference()).as_deref(),
            Some("Server=db;Password=hunter2")
        );
        assert_eq!(cache.count(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_zero_ttl_keeps_secrets_off_the_disk() {
        let dir = temp_dir("nottl");
        let cache = Cache::new(&dir, Duration::ZERO);

        cache.put(&reference(), "secret").unwrap();
        assert_eq!(cache.get(&reference()), None);
        assert!(!dir.join("secret-cache.json").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_expired_entry_is_a_miss() {
        let dir = temp_dir("expired");
        let cache = Cache::new(&dir, Duration::from_millis(1));

        cache.put(&reference(), "secret").unwrap();
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(cache.get(&reference()), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The reference is the AEAD's associated data, so an entry moved to
    /// another reference's key must fail to open rather than decrypt.
    #[test]
    fn an_entry_cannot_be_repointed_at_another_secret() {
        let dir = temp_dir("aad");
        let cache = Cache::new(&dir, DEFAULT_TTL);
        cache.put(&reference(), "the-real-dsn").unwrap();

        let other = Reference::parse("keyvault://kv/other", None).unwrap();
        let raw = std::fs::read_to_string(dir.join("secret-cache.json")).unwrap();
        let mut file: CacheFile = serde_json::from_str(&raw).unwrap();
        let entry = file.entries.values().next().unwrap().clone();
        file.entries.insert(entry_key(&other), entry);
        cache.save(&file).unwrap();

        assert_eq!(cache.get(&other), None, "moved entry must not decrypt");
        assert_eq!(cache.get(&reference()).as_deref(), Some("the-real-dsn"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_corrupt_cache_is_a_miss_not_an_error() {
        let dir = temp_dir("corrupt");
        let cache = Cache::new(&dir, DEFAULT_TTL);
        cache.put(&reference(), "secret").unwrap();

        std::fs::write(dir.join("secret-cache.json"), "{ not json").unwrap();
        assert_eq!(cache.get(&reference()), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clear_removes_the_key_as_well() {
        let dir = temp_dir("clear");
        let cache = Cache::new(&dir, DEFAULT_TTL);
        cache.put(&reference(), "secret").unwrap();
        assert!(dir.join("cache.key").exists());

        cache.clear().unwrap();
        assert!(!dir.join("secret-cache.json").exists());
        assert!(!dir.join("cache.key").exists());
        // Clearing twice is not an error.
        cache.clear().unwrap();

        let _ = std::fs::remove_dir_all(&dir);
    }
}
