//! Connection strings stored outside the config.
//!
//! A saved data source can hold a Key Vault reference instead of a credential,
//! so the config on disk need contain no secret at all. Resolving one is the
//! only thing between reading the config and opening a connection.

mod azure;
mod cache;
mod reference;

use std::time::Duration;

pub use cache::{Cache, DEFAULT_TTL};
pub use reference::{DEFAULT_SUFFIX, Reference, vault_only};

use crate::config::Config;
use crate::error::{Error, Result};

/// Turns a reference into a connection string, consulting the cache before the
/// vault.
pub struct Resolver {
    cache: Cache,
    /// Key Vault DNS suffix, for sovereign clouds.
    suffix: Option<String>,
}

impl Resolver {
    /// Builds a resolver over the binsql config directory, honouring
    /// `BINSQL_KEYVAULT_SUFFIX` and `BINSQL_SECRET_TTL` (seconds; `0` keeps
    /// secrets off the disk).
    pub fn from_env() -> Resolver {
        let ttl = std::env::var("BINSQL_SECRET_TTL")
            .ok()
            .and_then(|raw| raw.trim().parse::<u64>().ok())
            .map(Duration::from_secs)
            .unwrap_or(DEFAULT_TTL);

        let suffix = std::env::var("BINSQL_KEYVAULT_SUFFIX")
            .ok()
            .filter(|value| !value.trim().is_empty());

        Resolver {
            cache: Cache::new(config_dir(), ttl),
            suffix,
        }
    }

    pub fn new(cache: Cache, suffix: Option<String>) -> Resolver {
        Resolver { cache, suffix }
    }

    pub fn cache(&self) -> &Cache {
        &self.cache
    }

    /// Returns `dsn` unchanged when it is a literal connection string, or
    /// fetches the referenced secret when it is a reference.
    pub async fn resolve(&self, dsn: &str) -> Result<String> {
        if !Reference::is_reference(dsn) {
            // Catch the vault URL pasted from the portal, which is a reference
            // in spirit but names no secret — the driver's complaint about it
            // would be about hostnames and unhelpful.
            if let Some(host) = vault_only(dsn) {
                return Err(Error::config(anyhow::anyhow!(
                    "{dsn} names the vault {host} but not a secret in it. \
                     Use keyvault://{host}/<secret-name>."
                )));
            }
            return Ok(dsn.to_string());
        }

        let reference = Reference::parse(dsn, self.suffix.as_deref())?;

        if let Some(cached) = self.cache.get(&reference) {
            return Ok(cached);
        }

        let value = azure::fetch(&reference).await?;

        // A cache write failure must not break an otherwise successful
        // connection; the cost is refetching next time.
        let _ = self.cache.put(&reference, &value);

        Ok(value)
    }
}

impl Default for Resolver {
    fn default() -> Self {
        Resolver::from_env()
    }
}

/// The directory holding the config, the secret cache and its key.
pub fn config_dir() -> std::path::PathBuf {
    Config::path()
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolver(dir: &std::path::Path) -> Resolver {
        Resolver::new(Cache::new(dir, DEFAULT_TTL), None)
    }

    #[tokio::test]
    async fn a_literal_dsn_passes_through_untouched() {
        let dir = std::env::temp_dir().join("binsql-resolver-literal");
        let resolved = resolver(&dir)
            .resolve("postgres://app@localhost/app")
            .await
            .unwrap();
        assert_eq!(resolved, "postgres://app@localhost/app");
    }

    #[tokio::test]
    async fn a_cached_reference_never_reaches_the_vault() {
        let dir = std::env::temp_dir().join(format!("binsql-resolver-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let resolver = resolver(&dir);
        let reference = Reference::parse("keyvault://kv/dsn", None).unwrap();
        resolver
            .cache()
            .put(&reference, "Server=db;Database=app")
            .unwrap();

        // No `az` process is spawned; a miss here would try to run one and fail.
        let resolved = resolver.resolve("keyvault://kv/dsn").await.unwrap();
        assert_eq!(resolved, "Server=db;Database=app");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_vault_url_without_a_secret_says_what_is_missing() {
        let dir = std::env::temp_dir().join("binsql-resolver-vaultonly");
        let error = resolver(&dir)
            .resolve("https://kv-eimskip-prd.vault.azure.net/")
            .await
            .expect_err("should be rejected");
        let message = error.to_string();
        assert!(message.contains("keyvault://"), "{message}");
        assert!(message.contains("<secret-name>"), "{message}");
    }
}
