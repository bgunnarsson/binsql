//! Reading a secret out of Azure Key Vault, through the Azure CLI.
//!
//! v2 linked the Azure SDK and offered a credential chain — managed identity,
//! environment service principal, or the CLI. v3 shells out to `az` instead,
//! for two reasons: it is the same mechanism the SQL Server adapter already
//! uses for `fedauth=`, so binsql has exactly one Azure story to explain; and
//! the credential chain's other arms served CI, which is command mode's
//! territory and is not ported yet. When command mode returns and needs a
//! managed identity, this is the module that grows.

use crate::error::{Error, Result};

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

    let output = tokio::process::Command::new("az")
        .args(&args)
        // The Azure CLI's Python warnings go to stderr and have broken output
        // parsing before; silencing them is the documented workaround.
        .env("PYTHONWARNINGS", "ignore")
        .output()
        .await
        .map_err(|e| {
            Error::config(anyhow::anyhow!(
                "running `az`: {e}. Is the Azure CLI installed?"
            ))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::config(anyhow::anyhow!(
            "reading {reference}: {}",
            explain(stderr.trim())
        )));
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

/// Turns the CLI's messages into a next step. Being told to run `az login`
/// beats decoding an authentication dump.
fn explain(stderr: &str) -> String {
    let lower = stderr.to_ascii_lowercase();

    let hint = if lower.contains("az login")
        || lower.contains("please run")
        || lower.contains("no subscription")
        || lower.contains("refresh token")
        || lower.contains("aadsts")
    {
        Some("no usable Azure credential — run `az login`")
    } else if lower.contains("forbidden") || lower.contains("does not have secrets get permission")
    {
        Some(
            "the signed-in identity lacks 'get' on this secret — \
             grant it the Key Vault Secrets User role",
        )
    } else if lower.contains("secretnotfound") || lower.contains("was not found") {
        Some("check the secret name and the vault")
    } else if lower.contains("failed to resolve") || lower.contains("name or service not known") {
        Some("the vault host did not resolve — check the vault name")
    } else {
        None
    };

    match hint {
        Some(hint) => format!("{stderr}\n\n{hint}"),
        None => stderr.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_a_next_step_to_an_auth_failure() {
        let explained = explain("Please run 'az login' to setup account.");
        assert!(explained.contains("az login"));
    }

    #[test]
    fn adds_a_next_step_to_a_permission_failure() {
        let explained = explain("(Forbidden) Caller is not authorized to perform action");
        assert!(explained.contains("Key Vault Secrets User"));
    }

    /// The wording here is what `az` 2.89 actually printed for a vault that
    /// does not exist, not a guess at it.
    #[test]
    fn adds_a_next_step_to_an_unresolvable_vault() {
        let explained = explain(
            "ERROR: HTTPSConnection(host='no-such.vault.azure.net', port=443): \
             Failed to resolve 'no-such.vault.azure.net'",
        );
        assert!(explained.contains("check the vault name"), "{explained}");
    }

    #[test]
    fn passes_an_unrecognised_message_through_unchanged() {
        assert_eq!(explain("something odd happened"), "something odd happened");
    }
}
