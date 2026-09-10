//! Connection strings kept in the operating system's credential store.
//!
//! A data source whose `dsn` reads `keychain://eimskip/prod` holds no secret in
//! the config at all — the connection string itself lives in the macOS
//! Keychain, the Windows Credential Manager or the Secret Service, and the
//! config only names it. That is what makes a `connections.json` safe to commit
//! to a repository or hand to a colleague.
//!
//! The account is the connection's qualified name, so `eimskip/prod` is filed
//! under `eimskip/prod` and stays recognisable in Keychain Access.
//!
//! Unlike a Key Vault secret this one is already local, so it is fetched every
//! time rather than copied into the secret cache. A second place for the same
//! secret to sit is the thing this module exists to avoid.

use crate::error::{Error, Result};

/// The service every binsql secret is filed under.
pub const SERVICE: &str = "binsql";

/// What this operating system calls its credential store, so the UI can name
/// where a secret is actually going.
pub const STORE_NAME: &str = if cfg!(target_os = "macos") {
    "macOS Keychain"
} else if cfg!(target_os = "windows") {
    "Credential Manager"
} else {
    "Secret Service"
};

const SCHEME: &str = "keychain://";

/// Whether a DSN names a keychain entry rather than holding a connection
/// string. Cheap and does no I/O, so a caller can branch on it before deciding
/// to touch the store.
pub fn is_reference(dsn: &str) -> bool {
    account(dsn).is_some()
}

/// The account inside a `keychain://…` reference, or `None` when the DSN is not
/// one.
pub fn account(dsn: &str) -> Option<&str> {
    let trimmed = dsn.trim();
    // `get` rather than a slice: a DSN shorter than the scheme, or one whose
    // eleventh byte falls inside a multi-byte character, must answer no rather
    // than panic.
    let rest = trimmed
        .get(..SCHEME.len())
        .filter(|prefix| prefix.eq_ignore_ascii_case(SCHEME))
        .map(|prefix| &trimmed[prefix.len()..])?;
    Some(rest).filter(|account| !account.is_empty())
}

/// The reference a connection's secret is filed under.
pub fn reference(id: &str) -> String {
    format!("{SCHEME}{id}")
}

pub fn get(account: &str) -> Result<String> {
    match entry(account)?.get_password() {
        Ok(secret) => Ok(secret),
        Err(keyring::Error::NoEntry) => Err(Error::config(anyhow::anyhow!(
            "the {STORE_NAME} holds no {SERVICE} entry for {account}. \
             Edit the data source and enter its connection string again."
        ))),
        Err(err) => Err(failed("reading", account, err)),
    }
}

pub fn set(account: &str, secret: &str) -> Result<()> {
    entry(account)?
        .set_password(secret)
        .map_err(|err| failed("writing", account, err))
}

/// Removes the secret. An entry that is not there is not a failure: the config
/// is the record of what exists, and the store having already lost it changes
/// nothing about removing the connection.
pub fn delete(account: &str) -> Result<()> {
    match entry(account)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(failed("removing", account, err)),
    }
}

/// Refiles a secret under a new account, for when a connection is renamed.
pub fn rename(from: &str, to: &str) -> Result<()> {
    if from == to {
        return Ok(());
    }
    let secret = get(from)?;
    set(to, &secret)?;
    delete(from)
}

fn entry(account: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, account).map_err(|err| failed("opening", account, err))
}

fn failed(verb: &str, account: &str, err: keyring::Error) -> Error {
    Error::config(anyhow::anyhow!(
        "{verb} {SERVICE}/{account} in the {STORE_NAME}: {err}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only the parsing is tested here. Everything below `entry` talks to the
    /// real credential store, which a test run has no business writing to.
    #[test]
    fn recognises_a_reference_and_reads_its_account() {
        assert_eq!(account("keychain://eimskip/prod"), Some("eimskip/prod"));
        assert_eq!(account("  keychain://scratch  "), Some("scratch"));
        assert_eq!(account("KEYCHAIN://scratch"), Some("scratch"));

        assert!(!is_reference("keychain://"));
        assert!(!is_reference("postgres://localhost/app"));
        assert!(!is_reference("keyvault://kv/dsn"));
        // Short enough and multi-byte enough to slice badly.
        assert!(!is_reference("keych"));
        assert!(!is_reference("keychain:/€"));
    }

    #[test]
    fn a_reference_round_trips_through_the_qualified_name() {
        let id = "eimskip/prod";
        assert_eq!(account(&reference(id)), Some(id));
    }
}
