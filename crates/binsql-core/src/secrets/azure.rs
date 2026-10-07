//! Reading a secret out of Azure Key Vault, through the Azure CLI.
//!
//! v2 linked the Azure SDK and offered a credential chain — managed identity,
//! environment service principal, or the CLI. v3 shells out to `az` instead,
//! for two reasons: it is the same mechanism the SQL Server adapter already
//! uses for `fedauth=`, so binsql has exactly one Azure story to explain; and
//! the credential chain's other arms served CI, which is command mode's
//! territory and is not ported yet. When command mode returns and needs a
//! managed identity, this is the module that grows.

use crate::error::{Error, Reason, Result};

use super::reference::Reference;

/// Fetches the secret's current (or pinned) value.
pub async fn fetch(reference: &Reference) -> Result<String> {
    let mut args = vec![
        "keyvault".to_string(),
        "secret".to_string(),
        "show".to_string(),
        "--vault-name".to_string(),
        reference.vault_name().to_string(),
        "--name".to_string(),
        reference.name.clone(),
    ];
    if let Some(version) = &reference.version {
        args.push("--version".to_string());
        args.push(version.clone());
    }
    args.push("--output".to_string());
    args.push("json".to_string());

    let output = crate::az::output(&args).await.map_err(|e| Error::Secret {
        reference: reference.to_string(),
        reason: Some(Reason::AzMissing),
        hint: Some("install the Azure CLI".to_string()),
        detail: e.to_string(),
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(explain(reference, stderr.trim()));
    }

    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| Error::config(anyhow::anyhow!("parsing `az` output for {reference}: {e}")))?;

    let value = parsed
        .get("value")
        .and_then(|value| value.as_str())
        .ok_or_else(|| Error::config(anyhow::anyhow!("{reference} has no value")))?;

    if value.trim().is_empty() {
        return Err(Error::config(anyhow::anyhow!("{reference} is empty")));
    }
    Ok(value.to_string())
}

/// Turns the CLI's messages into a reason and a next step. Being told to run
/// `az login` beats decoding an authentication dump.
fn explain(reference: &Reference, stderr: &str) -> Error {
    let (reason, hint) = match classify(stderr) {
        Some((reason, hint)) => (Some(reason), Some(hint.to_string())),
        None => (None, None),
    };
    Error::Secret {
        reference: reference.to_string(),
        reason,
        hint,
        detail: stderr.to_string(),
    }
}

fn classify(stderr: &str) -> Option<(Reason, &'static str)> {
    let lower = stderr.to_ascii_lowercase();

    if lower.contains("az login")
        || lower.contains("please run")
        || lower.contains("no subscription")
        || lower.contains("refresh token")
        || lower.contains("aadsts")
    {
        Some((
            Reason::AzUnauthenticated,
            "no usable Azure credential — run `az login`",
        ))
    } else if lower.contains("forbidden") || lower.contains("does not have secrets get permission")
    {
        Some((
            Reason::VaultForbidden,
            "the signed-in identity lacks 'get' on this secret — \
             grant it the Key Vault Secrets User role",
        ))
    } else if lower.contains("secretnotfound") || lower.contains("was not found") {
        Some((
            Reason::SecretNotFound,
            "check the secret name and the vault",
        ))
    } else if lower.contains("failed to resolve") || lower.contains("name or service not known") {
        Some((
            Reason::VaultNotFound,
            "the vault host did not resolve — check the vault name",
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference() -> Reference {
        Reference::parse("keyvault://kv-test/db-dsn", None).unwrap()
    }

    fn reason(stderr: &str) -> Option<Reason> {
        classify(stderr).map(|(reason, _)| reason)
    }

    #[test]
    fn an_auth_failure_is_unauthenticated() {
        let stderr = "Please run 'az login' to setup account.";
        assert_eq!(reason(stderr), Some(Reason::AzUnauthenticated));
        assert!(
            explain(&reference(), stderr)
                .to_string()
                .contains("run `az login`")
        );
    }

    #[test]
    fn a_permission_failure_is_forbidden() {
        let stderr = "(Forbidden) Caller is not authorized to perform action";
        assert_eq!(reason(stderr), Some(Reason::VaultForbidden));
        assert!(
            explain(&reference(), stderr)
                .to_string()
                .contains("Key Vault Secrets User")
        );
    }

    #[test]
    fn a_missing_secret_is_not_found() {
        assert_eq!(
            reason(
                "(SecretNotFound) A secret with (name/id) db-dsn was not found in this key vault."
            ),
            Some(Reason::SecretNotFound)
        );
    }

    /// The wording here is what `az` 2.89 actually printed for a vault that
    /// does not exist, not a guess at it.
    #[test]
    fn an_unresolvable_vault_is_not_found() {
        let stderr = "ERROR: HTTPSConnection(host='no-such.vault.azure.net', port=443): \
                      Failed to resolve 'no-such.vault.azure.net'";
        assert_eq!(reason(stderr), Some(Reason::VaultNotFound));
        let explained = explain(&reference(), stderr).to_string();
        assert!(explained.contains("check the vault name"), "{explained}");
    }

    /// The text is the one `Error::Config` carried before the parts were
    /// kept apart.
    #[test]
    fn the_text_is_unchanged() {
        let stderr = "Please run 'az login' to setup account.";
        assert_eq!(
            explain(&reference(), stderr).to_string(),
            format!(
                "reading {}: {stderr}\n\nno usable Azure credential — run `az login`",
                reference()
            )
        );
    }

    #[test]
    fn passes_an_unrecognised_message_through_unchanged() {
        let explained = explain(&reference(), "something odd happened");
        assert!(
            matches!(&explained, Error::Secret { reason: None, hint: None, detail, .. } if detail == "something odd happened")
        );
        assert_eq!(
            explained.to_string(),
            format!("reading {}: something odd happened", reference())
        );
    }
}
