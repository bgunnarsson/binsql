use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    Sqlite,
    Postgres,
    MsSql,
    MySql,
}

impl Backend {
    pub const ALL: [Backend; 4] = [
        Backend::Sqlite,
        Backend::Postgres,
        Backend::MsSql,
        Backend::MySql,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Backend::Sqlite => "sqlite",
            Backend::Postgres => "postgres",
            Backend::MsSql => "mssql",
            Backend::MySql => "mysql",
        }
    }

    /// Display name for the UI.
    pub fn label(self) -> &'static str {
        match self {
            Backend::Sqlite => "SQLite",
            Backend::Postgres => "PostgreSQL",
            Backend::MsSql => "SQL Server",
            Backend::MySql => "MySQL",
        }
    }

    pub fn parse(s: &str) -> Result<Backend> {
        match s.trim().to_ascii_lowercase().as_str() {
            "sqlite" | "sqlite3" => Ok(Backend::Sqlite),
            "postgres" | "postgresql" | "pg" | "pgx" => Ok(Backend::Postgres),
            "mssql" | "sqlserver" | "azuresql" => Ok(Backend::MsSql),
            "mysql" | "mariadb" => Ok(Backend::MySql),
            _ => Err(Error::UnknownBackend(s.to_string())),
        }
    }

    /// Guesses the backend from the shape of a connection string. Deliberately
    /// conservative — `None` means "ask", not "invalid".
    pub fn infer(dsn: &str) -> Option<Backend> {
        let trimmed = dsn.trim();
        let lower = trimmed.to_ascii_lowercase();

        if lower.starts_with("postgres://") || lower.starts_with("postgresql://") {
            return Some(Backend::Postgres);
        }
        if lower.starts_with("sqlserver://") || lower.starts_with("azuresql://") {
            return Some(Backend::MsSql);
        }
        if lower.starts_with("mysql://") || lower.starts_with("mariadb://") {
            return Some(Backend::MySql);
        }
        if lower.starts_with("sqlite://") || lower.starts_with("file:") {
            return Some(Backend::Sqlite);
        }

        // Key=value connection strings are SQL Server's native form.
        if lower.contains("fedauth=") || (lower.contains("server=") && lower.contains(';')) {
            return Some(Backend::MsSql);
        }

        // go-sql-driver form carried over from saved v2 profiles:
        // user:pass@tcp(host:port)/db
        if is_go_mysql_dsn(trimmed) {
            return Some(Backend::MySql);
        }

        // libpq keyword form.
        if lower.contains("dbname=") || lower.contains("sslmode=") {
            return Some(Backend::Postgres);
        }

        if matches!(
            Path::new(trimmed)
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some("db" | "sqlite" | "sqlite3" | "db3")
        ) {
            return Some(Backend::Sqlite);
        }

        if !trimmed.is_empty() && !trimmed.contains("://") && Path::new(trimmed).is_file() {
            return Some(Backend::Sqlite);
        }

        None
    }

    pub fn dialect(self) -> Dialect {
        Dialect { backend: self }
    }

    /// Whether one connection can see sibling databases on the same server.
    /// Postgres cannot — reading another database needs a separate connection,
    /// which is why the tree opens catalogs lazily.
    pub fn catalogs_share_connection(self) -> bool {
        match self {
            Backend::Postgres | Backend::Sqlite => false,
            Backend::MySql | Backend::MsSql => true,
        }
    }

    /// Whether the backend has a real schema level between database and table.
    /// MySQL does not: its "schema" is the database.
    pub fn has_schemas(self) -> bool {
        matches!(self, Backend::Postgres | Backend::MsSql)
    }
}

impl fmt::Display for Backend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

fn is_go_mysql_dsn(dsn: &str) -> bool {
    let Some(at) = dsn.rfind('@') else {
        return false;
    };
    let (creds, rest) = dsn.split_at(at);
    if creds.contains(['/', ' ']) {
        return false;
    }
    let rest = &rest[1..];
    (rest.starts_with("tcp(") || rest.starts_with("unix(")) && rest.contains(')')
}

/// The per-backend SQL spelling the generic layers need. Everything else about
/// a backend lives in its adapter.
#[derive(Debug, Clone, Copy)]
pub struct Dialect {
    pub backend: Backend,
}

impl Dialect {
    /// Quotes one identifier part, escaping any embedded quote character.
    pub fn quote_ident(&self, ident: &str) -> String {
        match self.backend {
            Backend::MySql => format!("`{}`", ident.replace('`', "``")),
            Backend::MsSql => format!("[{}]", ident.replace(']', "]]")),
            Backend::Postgres | Backend::Sqlite => format!("\"{}\"", ident.replace('"', "\"\"")),
        }
    }

    /// Quotes a possibly qualified name, part by part.
    pub fn quote_qualified(&self, parts: &[&str]) -> String {
        parts
            .iter()
            .filter(|p| !p.is_empty())
            .map(|p| self.quote_ident(p))
            .collect::<Vec<_>>()
            .join(".")
    }

    /// A row-limited `SELECT *`. SQL Server predates `LIMIT`, so it gets `TOP`.
    pub fn select_limit(&self, table: &str, limit: usize) -> String {
        match self.backend {
            Backend::MsSql => format!("SELECT TOP {limit} * FROM {table}"),
            _ => format!("SELECT * FROM {table} LIMIT {limit}"),
        }
    }

    /// Wraps a user query so the server, not binsql, discards the extra rows.
    /// Returns `None` when the statement already limits itself or is not a
    /// query we can safely nest.
    pub fn wrap_limit(&self, sql: &str, limit: usize) -> Option<String> {
        let normalized = sql.trim().trim_end_matches(';').trim();
        let lower = normalized.to_ascii_lowercase();
        if !lower.starts_with("select") && !lower.starts_with("with") {
            return None;
        }
        if lower.contains(" limit ") || lower.contains("\nlimit ") || lower.contains(" top ") {
            return None;
        }
        match self.backend {
            Backend::MsSql => None,
            _ => Some(format!(
                "SELECT * FROM ({normalized}) AS binsql_q LIMIT {limit}"
            )),
        }
    }
}
