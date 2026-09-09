mod mssql;
mod mysql;
mod postgres;
mod sqlite;
mod sqlx_common;

use async_trait::async_trait;

use crate::backend::Backend;
use crate::error::Result;
use crate::schema::{Catalog, ObjectRef};
use crate::value::{Column, ResultSet};

/// One live connection to one database, with every backend difference already
/// resolved. Nothing above this trait knows which engine it is talking to,
/// beyond asking [`Adapter::backend`] for a label or a dialect.
#[async_trait]
pub trait Adapter: Send + Sync {
    fn backend(&self) -> Backend;

    /// The server's own version banner, for the connection header.
    fn server_version(&self) -> Option<&str>;

    /// The database this connection is attached to.
    fn current_catalog(&self) -> Option<&str>;

    async fn catalogs(&self) -> Result<Vec<Catalog>>;

    async fn schemas(&self, catalog: &str) -> Result<Vec<String>>;

    /// Tables and views in one schema, sorted by kind then name.
    async fn objects(&self, catalog: &str, schema: Option<&str>) -> Result<Vec<ObjectRef>>;

    async fn columns(&self, object: &ObjectRef) -> Result<Vec<Column>>;

    /// Runs one statement. `limit` caps the rows pulled off the wire; the
    /// result is flagged truncated when the cap was reached.
    async fn run(&self, sql: &str, limit: Option<usize>) -> Result<ResultSet>;

    /// Opens a second connection when `catalog` cannot be reached from this
    /// one, and returns `None` when it can. Postgres is the reason this exists:
    /// its databases are not cross-readable on a single connection.
    async fn open_catalog(&self, catalog: &str) -> Result<Option<Box<dyn Adapter>>>;
}

/// Opens a connection. `backend` may be inferred from the DSN by
/// [`Backend::infer`] before calling this.
pub async fn connect(backend: Backend, dsn: &str) -> Result<Box<dyn Adapter>> {
    match backend {
        Backend::Sqlite => Ok(Box::new(sqlite::SqliteAdapter::connect(dsn).await?)),
        Backend::Postgres => Ok(Box::new(postgres::PostgresAdapter::connect(dsn).await?)),
        Backend::MySql => Ok(Box::new(mysql::MySqlAdapter::connect(dsn).await?)),
        Backend::MsSql => Ok(Box::new(mssql::MsSqlAdapter::connect(dsn).await?)),
    }
}

/// Whether a statement is expected to return rows.
///
/// The sqlx adapters do not need this — their result stream reports which it
/// got — but tiberius takes different calls for the two cases, and the
/// read-only guard wants the answer before anything is sent.
pub fn returns_rows(sql: &str) -> bool {
    let head = sql
        .trim_start()
        .trim_start_matches(|c: char| c == '(' || c.is_whitespace());
    let word: String = head
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .to_ascii_lowercase();

    match word.as_str() {
        "select" | "with" | "show" | "pragma" | "explain" | "describe" | "desc" | "values"
        | "table" => true,
        // `INSERT … RETURNING` and friends do return rows.
        "insert" | "update" | "delete" => sql.to_ascii_lowercase().contains("returning"),
        _ => false,
    }
}

/// Whether a statement writes. Used by the read-only guard, which refuses
/// before opening a transaction rather than rolling one back.
pub fn is_mutating(sql: &str) -> bool {
    let head = sql.trim_start();
    let word: String = head
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .to_ascii_lowercase();

    matches!(
        word.as_str(),
        "insert"
            | "update"
            | "delete"
            | "drop"
            | "truncate"
            | "alter"
            | "create"
            | "replace"
            | "merge"
            | "grant"
            | "revoke"
            | "call"
            | "exec"
            | "execute"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_row_returning_statements() {
        assert!(returns_rows("SELECT 1"));
        assert!(returns_rows("  with x as (select 1) select * from x"));
        assert!(returns_rows("(SELECT 1)"));
        assert!(returns_rows("INSERT INTO t VALUES (1) RETURNING id"));
        assert!(!returns_rows("INSERT INTO t VALUES (1)"));
        assert!(!returns_rows("UPDATE t SET a = 1"));
    }

    #[test]
    fn classifies_writes() {
        assert!(is_mutating("DELETE FROM t"));
        assert!(is_mutating("  truncate table t"));
        assert!(!is_mutating("SELECT * FROM t"));
        assert!(!is_mutating("EXPLAIN SELECT 1"));
    }
}
