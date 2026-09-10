//! Reading the safe parts of a connection string.
//!
//! A DSN carries a password as often as not, so nothing here returns anything
//! it has not positively identified as a host. Every path fails closed: an
//! unfamiliar shape yields `None` rather than a guess, because the cost of
//! guessing wrong is a credential on someone's screen.

use crate::Backend;
use crate::secrets::Reference;

/// The server a DSN points at, for showing on screen — `db.prod.internal:5432`,
/// or the file name for SQLite.
///
/// `None` when it cannot be read out safely: a secret reference, a shape this
/// does not recognise, or anything that came back still looking like it might
/// hold a credential.
pub fn display_host(backend: Backend, dsn: &str) -> Option<String> {
    let dsn = dsn.trim();
    // A reference names a secret rather than holding one, so the connection
    // string it stands for is not here to read.
    if Reference::is_reference(dsn) {
        return None;
    }

    let host = match backend {
        Backend::Sqlite => file_name(dsn),
        _ if dsn.contains("://") => authority(dsn),
        // Key=value is SQL Server's native form, and the only one left worth
        // reading.
        _ => keyword_server(dsn),
    }?;

    safe(&host)
}

/// The host of a URL-shaped DSN, with any credentials dropped.
fn authority(dsn: &str) -> Option<String> {
    let (_, rest) = dsn.split_once("://")?;
    let authority = rest
        .split(['/', '?'])
        .next()
        .filter(|part| !part.is_empty())?;
    // The last `@`, not the first: a password may contain one.
    let host = match authority.rsplit_once('@') {
        Some((_, host)) => host,
        None => authority,
    };
    Some(host.to_string())
}

/// The database file a SQLite DSN names, without the directories leading to it.
fn file_name(dsn: &str) -> Option<String> {
    let path = dsn
        .strip_prefix("sqlite://")
        .or_else(|| dsn.strip_prefix("file:"))
        .unwrap_or(dsn);
    let path = path.split('?').next().unwrap_or(path);
    if path.is_empty() {
        return None;
    }
    // An in-memory database has no file, and saying so is the honest answer.
    if path.contains(":memory:") {
        return Some("in-memory".to_string());
    }
    Some(
        path.rsplit(['/', '\\'])
            .next()
            .filter(|name| !name.is_empty())?
            .to_string(),
    )
}

/// The `Server` of a `key=value;key=value` connection string. The password
/// lives under its own key, so the server's value never contains one.
fn keyword_server(dsn: &str) -> Option<String> {
    for pair in dsn.split(';') {
        // A trailing `;` and any stray fragment simply are not the key being
        // looked for.
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        if key != "server" && key != "data source" && key != "address" {
            continue;
        }
        // `tcp:` is the protocol prefix SQL Server writes, not part of the
        // name anybody reads.
        let value = value.trim();
        let value = value.strip_prefix("tcp:").unwrap_or(value);
        // `host,1433` is that dialect's spelling of a port.
        return Some(value.replace(',', ":"));
    }
    None
}

/// What may be shown. A host is a name and a port and nothing else, so
/// anything carrying the punctuation of a connection string is refused rather
/// than trimmed — a trim that misses leaks the secret it was meant to remove.
fn safe(host: &str) -> Option<String> {
    const LONGEST: usize = 64;

    if host.is_empty() || host.len() > LONGEST {
        return None;
    }
    if host
        .chars()
        .any(|ch| ch.is_whitespace() || matches!(ch, '@' | '=' | ';' | '\'' | '"'))
    {
        return None;
    }
    Some(host.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_url_keeps_the_host_and_port() {
        assert_eq!(
            display_host(Backend::Postgres, "postgres://db.internal:5432/app"),
            Some("db.internal:5432".to_string())
        );
        assert_eq!(
            display_host(Backend::MySql, "mysql://localhost/shop?ssl=true"),
            Some("localhost".to_string())
        );
    }

    #[test]
    fn credentials_never_survive() {
        assert_eq!(
            display_host(
                Backend::Postgres,
                "postgres://user:hunter2@db.internal:5432/app"
            ),
            Some("db.internal:5432".to_string())
        );
        // A password may hold an `@` of its own, so the host is what follows
        // the last one, not the first.
        assert_eq!(
            display_host(
                Backend::Postgres,
                "postgres://user:p@ssw0rd@db.internal/app"
            ),
            Some("db.internal".to_string())
        );
    }

    #[test]
    fn sqlite_shows_the_file_not_the_path() {
        assert_eq!(
            display_host(Backend::Sqlite, "/Users/someone/work/demo.db"),
            Some("demo.db".to_string())
        );
        assert_eq!(
            display_host(Backend::Sqlite, "sqlite://./demo.db?mode=ro"),
            Some("demo.db".to_string())
        );
        assert_eq!(
            display_host(Backend::Sqlite, "sqlite://:memory:"),
            Some("in-memory".to_string())
        );
    }

    #[test]
    fn sql_servers_keyword_form_is_read_without_its_password() {
        let host = display_host(
            Backend::MsSql,
            "Server=tcp:sql.internal,1433;Database=app;User Id=sa;Password=hunter2;",
        );
        assert_eq!(host, Some("sql.internal:1433".to_string()));
    }

    #[test]
    fn a_secret_reference_has_no_host_to_show() {
        assert_eq!(
            display_host(Backend::Postgres, "keyvault://my-vault/prod-dsn"),
            None
        );
    }

    #[test]
    fn an_unreadable_dsn_shows_nothing_rather_than_a_guess() {
        assert_eq!(display_host(Backend::Postgres, "something odd"), None);
        assert_eq!(display_host(Backend::MsSql, "Password=hunter2"), None);
    }
}
