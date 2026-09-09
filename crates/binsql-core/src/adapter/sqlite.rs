use std::str::FromStr;

use async_trait::async_trait;
use sqlx::Row;
use sqlx::sqlite::{
    SqliteConnectOptions, SqlitePool, SqlitePoolOptions, SqliteQueryResult, SqliteRow,
};

use super::Adapter;
use super::sqlx_common::{self, decode_as, decode_fallback};
use crate::backend::Backend;
use crate::error::{Error, Result};
use crate::schema::{Catalog, ObjectKind, ObjectRef};
use crate::value::{Column, ResultSet, Value};

pub struct SqliteAdapter {
    pool: SqlitePool,
    version: Option<String>,
}

impl SqliteAdapter {
    pub async fn connect(dsn: &str) -> Result<Self> {
        let options = connect_options(dsn)?;

        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await
            .map_err(|e| Error::connect(dsn, e))?;

        let version: Option<String> = sqlx::query_scalar("SELECT sqlite_version()")
            .fetch_one(&pool)
            .await
            .ok();

        Ok(SqliteAdapter { pool, version })
    }
}

fn connect_options(dsn: &str) -> Result<SqliteConnectOptions> {
    let trimmed = dsn.trim();
    if trimmed.starts_with("sqlite:") || trimmed.starts_with("file:") {
        return SqliteConnectOptions::from_str(trimmed).map_err(|e| Error::connect(dsn, e));
    }
    // A bare path. `create_if_missing` stays off: opening a database binsql
    // just invented is never what someone meant by a typo'd filename.
    Ok(SqliteConnectOptions::new().filename(trimmed))
}

#[async_trait]
impl Adapter for SqliteAdapter {
    fn backend(&self) -> Backend {
        Backend::Sqlite
    }

    fn server_version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    fn current_catalog(&self) -> Option<&str> {
        Some("main")
    }

    async fn catalogs(&self) -> Result<Vec<Catalog>> {
        let rows = sqlx::query("PRAGMA database_list")
            .fetch_all(&self.pool)
            .await
            .map_err(Error::query)?;

        Ok(rows
            .iter()
            .filter_map(|row| row.try_get::<String, _>("name").ok())
            .map(|name| Catalog {
                is_current: name == "main",
                name,
            })
            .collect())
    }

    async fn schemas(&self, _catalog: &str) -> Result<Vec<String>> {
        // SQLite has no schema level; its attached databases are the catalogs.
        Ok(Vec::new())
    }

    async fn objects(&self, catalog: &str, _schema: Option<&str>) -> Result<Vec<ObjectRef>> {
        // sqlite_master lives inside each attached database and cannot be
        // reached with a bind parameter, so the catalog is quoted in.
        let master = Backend::Sqlite
            .dialect()
            .quote_qualified(&[catalog, "sqlite_master"]);
        let sql = format!(
            "SELECT name, type FROM {master} \
             WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\' \
             ORDER BY type, name"
        );

        let rows = sqlx::query(&sql)
            .fetch_all(&self.pool)
            .await
            .map_err(Error::query)?;

        Ok(rows
            .iter()
            .filter_map(|row| {
                let name: String = row.try_get("name").ok()?;
                let kind = match row.try_get::<String, _>("type").ok()?.as_str() {
                    "view" => ObjectKind::View,
                    _ => ObjectKind::Table,
                };
                Some(ObjectRef::new(Some(catalog.to_string()), None, name, kind))
            })
            .collect())
    }

    async fn columns(&self, object: &ObjectRef) -> Result<Vec<Column>> {
        let catalog = object.catalog.as_deref().unwrap_or("main");
        let rows = sqlx::query(
            "SELECT name, type, \"notnull\", dflt_value, pk FROM pragma_table_info(?, ?)",
        )
        .bind(&object.name)
        .bind(catalog)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::query)?;

        Ok(rows
            .iter()
            .filter_map(|row| {
                let name: String = row.try_get("name").ok()?;
                let type_name: String = row.try_get("type").unwrap_or_default();
                Some(Column {
                    name,
                    type_name,
                    nullable: Some(row.try_get::<i64, _>("notnull").unwrap_or(0) == 0),
                    default: row
                        .try_get::<Option<String>, _>("dflt_value")
                        .ok()
                        .flatten(),
                    primary_key: row.try_get::<i64, _>("pk").unwrap_or(0) > 0,
                })
            })
            .collect())
    }

    async fn run(&self, sql: &str, limit: Option<usize>) -> Result<ResultSet> {
        sqlx_common::run(&self.pool, sql, limit, decode, affected).await
    }

    async fn open_catalog(&self, _catalog: &str) -> Result<Option<Box<dyn Adapter>>> {
        // Attached databases are reachable from the connection that attached
        // them, so there is never a second connection to open.
        Ok(None)
    }
}

fn affected(result: &SqliteQueryResult) -> u64 {
    result.rows_affected()
}

/// SQLite stores a value's type per row, not per column, so a declared type is
/// a hint rather than a guarantee — every branch here falls through to the
/// generic chain if the hint turns out to be wrong.
fn decode(row: &SqliteRow, idx: usize) -> Value {
    let type_name = column_type(row, idx);

    match type_name.as_str() {
        "INTEGER" | "INT" | "BIGINT" => decode_as!(row, idx, i64, Value::Int),
        "REAL" | "DOUBLE" | "FLOAT" => decode_as!(row, idx, f64, Value::Float),
        "BOOLEAN" | "BOOL" => decode_as!(row, idx, bool, Value::Bool),
        "BLOB" => decode_as!(row, idx, Vec<u8>, Value::Bytes),
        "TEXT" | "VARCHAR" | "CHAR" | "CLOB" => decode_as!(row, idx, String, Value::Text),
        "DATETIME" | "TIMESTAMP" | "DATE" | "TIME" => {
            decode_as!(row, idx, String, Value::Timestamp)
        }
        _ => {}
    }

    decode_as!(row, idx, i64, Value::Int);
    decode_as!(row, idx, f64, Value::Float);
    decode_fallback!(row, idx, type_name)
}

fn column_type(row: &SqliteRow, idx: usize) -> String {
    use sqlx::{Column as _, TypeInfo as _};
    row.columns()
        .get(idx)
        .map(|c| c.type_info().name().to_ascii_uppercase())
        .unwrap_or_default()
}
