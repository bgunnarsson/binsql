//! Parsing the reference forms a v2 config can contain.
//!
//! A reference names a secret; it never contains one, so a `Reference` is safe
//! to put in an error message or a log line.

use crate::error::{Error, Result};

/// The Key Vault DNS suffix for the public Azure cloud. Sovereign clouds
/// override it with `BINSQL_KEYVAULT_SUFFIX`.
pub const DEFAULT_SUFFIX: &str = "vault.azure.net";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    /// Fully-qualified vault host, e.g. `my-vault.vault.azure.net`.
    pub vault_host: String,
    pub name: String,
    /// Empty means the current version.
    pub version: Option<String>,
}

impl Reference {
    /// Whether a DSN names a secret rather than holding a connection string.
    /// Cheap and does no I/O, so a caller can branch on it before deciding to
    /// authenticate.
    pub fn is_reference(dsn: &str) -> bool {
        let lower = dsn.trim().to_ascii_lowercase();
        lower.starts_with("keyvault://")
            || lower.starts_with("azkv://")
            // The secret identifier as copied from the Azure portal.
            || (lower.starts_with("https://") && lower.contains("/secrets/"))
    }

    /// Accepted forms:
    ///
    /// ```text
    /// keyvault://my-vault/secret-name
    /// keyvault://my-vault/secret-name/version
    /// keyvault://my-vault.vault.azure.net/secret-name
    /// https://my-vault.vault.azure.net/secrets/secret-name[/version]
    /// ```
    pub fn parse(dsn: &str, suffix: Option<&str>) -> Result<Reference> {
        let raw = dsn.trim();
        let suffix = suffix.filter(|s| !s.is_empty()).unwrap_or(DEFAULT_SUFFIX);

        let (scheme, rest) = raw
            .split_once("://")
            .ok_or_else(|| reference_error(raw, "it has no scheme"))?;
        let (host, path) = match rest.split_once('/') {
            Some((host, path)) => (host, path),
            None => (rest, ""),
        };
        let parts = split_path(path);

        match scheme.to_ascii_lowercase().as_str() {
            "keyvault" | "azkv" => {
                if host.is_empty() {
                    return Err(reference_error(raw, "it is missing a vault name"));
                }
                // A bare name gets the cloud's DNS suffix; a dotted name is
                // already a host.
                let vault_host = if host.contains('.') {
                    host.to_string()
                } else {
                    format!("{host}.{suffix}")
                };

                match parts.len() {
                    1 => Ok(Reference {
                        vault_host,
                        name: parts[0].to_string(),
                        version: None,
                    }),
                    2 => Ok(Reference {
                        vault_host,
                        name: parts[0].to_string(),
                        version: Some(parts[1].to_string()),
                    }),
                    _ => Err(reference_error(
                        raw,
                        "it should look like keyvault://<vault>/<secret>[/<version>]",
                    )),
                }
            }

            "https" => {
                if parts.len() < 2 || !parts[0].eq_ignore_ascii_case("secrets") {
                    return Err(reference_error(
                        raw,
                        "it should look like https://<vault-host>/secrets/<secret>[/<version>]",
                    ));
                }
                Ok(Reference {
                    vault_host: host.to_string(),
                    name: parts[1].to_string(),
                    version: parts.get(2).map(|v| (*v).to_string()),
                })
            }

            _ => Err(reference_error(raw, "it is not a secret reference")),
        }
    }

    /// The short vault name, which is what `az` wants for `--vault-name`.
    pub fn vault_name(&self) -> &str {
        let lower = self.vault_host.to_ascii_lowercase();
        if let Some(at) = lower.find(".vault.")
            && at > 0
        {
            return &self.vault_host[..at];
        }
        match self.vault_host.find('.') {
            Some(at) if at > 0 => &self.vault_host[..at],
            _ => &self.vault_host,
        }
    }

    /// The canonical spelling. This is the cache's associated data and its key
    /// material, so it must match v2 byte for byte.
    pub fn canonical(&self) -> String {
        match &self.version {
            Some(version) => format!("keyvault://{}/{}/{version}", self.vault_host, self.name),
            None => format!("keyvault://{}/{}", self.vault_host, self.name),
        }
    }
}

impl std::fmt::Display for Reference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.canonical())
    }
}

/// Whether a DSN names a vault without identifying a secret in it — the easy
/// mistake of pasting the vault's URL straight out of the portal. Returns the
/// host so the caller can spell out the complete form.
pub fn vault_only(dsn: &str) -> Option<String> {
    let raw = dsn.trim();
    let (scheme, rest) = raw.split_once("://")?;
    let (host, path) = match rest.split_once('/') {
        Some((host, path)) => (host, path),
        None => (rest, ""),
    };
    let parts = split_path(path);

    match scheme.to_ascii_lowercase().as_str() {
        "https" => {
            if !host.to_ascii_lowercase().contains(".vault.") {
                return None;
            }
            // A complete identifier carries /secrets/<name>.
            if parts.len() >= 2 && parts[0].eq_ignore_ascii_case("secrets") {
                return None;
            }
            Some(host.to_string())
        }
        "keyvault" | "azkv" => {
            if host.is_empty() || !parts.is_empty() {
                return None;
            }
            Some(if host.contains('.') {
                host.to_string()
            } else {
                format!("{host}.{DEFAULT_SUFFIX}")
            })
        }
        _ => None,
    }
}

fn split_path(path: &str) -> Vec<&str> {
    path.trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn reference_error(raw: &str, why: &str) -> Error {
    Error::config(anyhow::anyhow!("secret reference {raw:?}: {why}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_the_reference_forms() {
        assert!(Reference::is_reference("keyvault://v/s"));
        assert!(Reference::is_reference("azkv://v/s"));
        assert!(Reference::is_reference(
            "https://v.vault.azure.net/secrets/s"
        ));
        assert!(!Reference::is_reference("postgres://localhost/app"));
        assert!(!Reference::is_reference("https://example.com/app"));
    }

    #[test]
    fn adds_the_dns_suffix_to_a_bare_vault_name() {
        let reference = Reference::parse("keyvault://kv-eimskip-prd/dsn", None).unwrap();
        assert_eq!(reference.vault_host, "kv-eimskip-prd.vault.azure.net");
        assert_eq!(reference.name, "dsn");
        assert_eq!(reference.version, None);
        assert_eq!(reference.vault_name(), "kv-eimskip-prd");
    }

    #[test]
    fn keeps_a_dotted_host_as_given() {
        let reference = Reference::parse("azkv://v.vault.usgovcloudapi.net/s/42", None).unwrap();
        assert_eq!(reference.vault_host, "v.vault.usgovcloudapi.net");
        assert_eq!(reference.version.as_deref(), Some("42"));
    }

    #[test]
    fn honours_a_sovereign_cloud_suffix() {
        let reference =
            Reference::parse("keyvault://v/s", Some("vault.usgovcloudapi.net")).unwrap();
        assert_eq!(reference.vault_host, "v.vault.usgovcloudapi.net");
    }

    #[test]
    fn parses_a_portal_secret_identifier() {
        let reference =
            Reference::parse("https://kv.vault.azure.net/secrets/my-dsn/abc123", None).unwrap();
        assert_eq!(reference.vault_host, "kv.vault.azure.net");
        assert_eq!(reference.name, "my-dsn");
        assert_eq!(reference.version.as_deref(), Some("abc123"));
    }

    #[test]
    fn rejects_a_reference_with_too_many_segments() {
        assert!(Reference::parse("keyvault://v/s/1/2", None).is_err());
    }

    /// The cache is shared with v2, so this spelling is a compatibility
    /// contract rather than a formatting choice.
    #[test]
    fn canonical_form_matches_v2() {
        let reference = Reference::parse("keyvault://kv/dsn", None).unwrap();
        assert_eq!(reference.canonical(), "keyvault://kv.vault.azure.net/dsn");

        let versioned = Reference::parse("keyvault://kv/dsn/v1", None).unwrap();
        assert_eq!(
            versioned.canonical(),
            "keyvault://kv.vault.azure.net/dsn/v1"
        );
    }

    #[test]
    fn spots_a_vault_url_with_no_secret() {
        assert_eq!(
            vault_only("https://kv.vault.azure.net/"),
            Some("kv.vault.azure.net".to_string())
        );
        assert_eq!(
            vault_only("keyvault://kv"),
            Some("kv.vault.azure.net".to_string())
        );
        assert_eq!(vault_only("https://kv.vault.azure.net/secrets/s"), None);
        assert_eq!(vault_only("keyvault://kv/s"), None);
    }
}
