mod mssql;
mod mysql;
mod postgres;
mod sqlite;
mod sqlx_common;

use async_trait::async_trait;

use crate::backend::Backend;
use crate::error::Result;
use crate::schema::{Catalog, ObjectRef};
use crate::sql;
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
/// got — but tiberius takes different calls for the two cases and has to
/// choose before anything is sent.
pub fn returns_rows(sql: &str, backend: Backend) -> bool {
    // A parenthesised `(SELECT …)` is still a select; the lexer reads the
    // paren as code and the keyword after it as the first word.
    match sql::classify(sql, backend) {
        sql::Kind::Read => true,
        // `INSERT … RETURNING` and friends do return rows.
        sql::Kind::Write => sql::has_keyword(sql, backend, "RETURNING"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_row_returning_statements() {
        let mssql = Backend::MsSql;
        assert!(returns_rows("SELECT 1", mssql));
        assert!(returns_rows(
            "  with x as (select 1) select * from x",
            mssql
        ));
        assert!(returns_rows("(SELECT 1)", mssql));
        assert!(returns_rows("INSERT INTO t VALUES (1) RETURNING id", mssql));
        assert!(!returns_rows("INSERT INTO t VALUES (1)", mssql));
        assert!(!returns_rows("UPDATE t SET a = 1", mssql));
        // A comment used to hide the keyword from the old first-word reading.
        assert!(returns_rows("/* daily */ SELECT 1", mssql));
    }
}
