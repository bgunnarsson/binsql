use std::str::FromStr;

use async_trait::async_trait;
use sqlx::Row;
use sqlx::mysql::{
    MySql, MySqlConnectOptions, MySqlPool, MySqlPoolOptions, MySqlQueryResult, MySqlRow,
    MySqlTypeInfo,
};
use tokio_util::sync::CancellationToken;

use super::Adapter;
use super::sqlx_common::{self, Binding, Codec, bind_as_given, decode_as, decode_fallback};
use crate::backend::Backend;
use crate::error::{Error, Result};
use crate::schema::{Catalog, ObjectKind, ObjectRef};
use crate::sql::Bound;
use crate::value::{Column, ResultSet, Value};

const CODEC: Codec<MySql> = Codec {
    decode,
    affected,
    bind,
    describe: false,
};

pub struct MySqlAdapter {
    pool: MySqlPool,
    version: Option<String>,
    database: Option<String>,
}

impl MySqlAdapter {
    pub async fn connect(dsn: &str) -> Result<Self> {
        let pool = MySqlPoolOptions::new()
            .max_connections(4)
            .connect_with(connect_options(dsn)?)
            .await
            .map_err(|e| Error::connect(dsn, e))?;

        let version: Option<String> = sqlx::query_scalar("SELECT VERSION()")
            .fetch_one(&pool)
            .await
            .ok();
        let database: Option<String> = sqlx::query_scalar("SELECT DATABASE()")
            .fetch_one(&pool)
            .await
            .ok()
            .flatten();

        Ok(MySqlAdapter {
            pool,
            version,
            database,
        })
    }

    /// Runs the statement on a connection of its own so that a cancel has
    /// something to name: `KILL QUERY` stops the statement running on one
    /// connection and has to be sent from another.
    ///
    /// It sits outside the `Adapter` impl because a boxed trait future cannot
    /// hold a borrow of a pooled connection.
    async fn run_on_own_connection(
        &self,
        statement: &Bound,
        limit: Option<usize>,
        cancel: &CancellationToken,
    ) -> Result<ResultSet> {
        let mut connection = self.pool.acquire().await.map_err(Error::query)?;
        // One small round-trip per statement, and the price of being able to
        // stop the large one that follows it.
        let id: Option<u64> = sqlx::query_scalar("SELECT CONNECTION_ID()")
            .fetch_one(&mut *connection)
            .await
            .ok();

        let result =
            sqlx_common::run::<MySql>(&mut connection, statement, limit, &CODEC, cancel).await;

        if matches!(result, Err(Error::Cancelled))
            && let Some(id) = id
        {
            // `KILL QUERY` ends the statement and leaves the connection open.
            // The id is a number MySQL gave us, and the statement takes no
            // placeholder anyway.
            let _ = sqlx::raw_sql(&format!("KILL QUERY {id}"))
                .execute(&self.pool)
                .await;
        }

        result
    }
}

/// Accepts both the URL form sqlx wants and the `go-sql-driver` form that v2
/// profiles are written in.
fn connect_options(dsn: &str) -> Result<MySqlConnectOptions> {
    let trimmed = dsn.trim();
    if trimmed.contains("://") {
        return MySqlConnectOptions::from_str(trimmed).map_err(|e| Error::connect(dsn, e));
    }
    parse_go_dsn(trimmed).ok_or_else(|| Error::UnknownBackend(dsn.to_string()))
}

/// `user:pass@tcp(host:port)/dbname?params` — the Go driver's own spelling.
/// Its query parameters are Go-specific tuning and are dropped rather than
/// mistranslated.
fn parse_go_dsn(dsn: &str) -> Option<MySqlConnectOptions> {
    let (credentials, rest) = match dsn.rfind('@') {
        Some(at) => (&dsn[..at], &dsn[at + 1..]),
        None => ("", dsn),
    };

    let open = rest.find('(')?;
    let close = rest.find(')')?;
    let protocol = &rest[..open];
    let address = &rest[open + 1..close];
    let tail = &rest[close + 1..];

    let mut options = MySqlConnectOptions::new();

    if !credentials.is_empty() {
        let (user, password) = match credentials.split_once(':') {
            Some((user, password)) => (user, Some(password)),
            None => (credentials, None),
        };
        options = options.username(user);
        if let Some(password) = password {
            options = options.password(password);
        }
    }

    if protocol == "unix" {
        options = options.socket(address);
    } else {
        let (host, port) = match address.rsplit_once(':') {
            Some((host, port)) => (host, port.parse().ok()),
            None => (address, None),
        };
        options = options.host(host);
        if let Some(port) = port {
            options = options.port(port);
        }
    }

    let database = tail
        .trim_start_matches('/')
        .split('?')
        .next()
        .unwrap_or_default();
    if !database.is_empty() {
        options = options.database(database);
    }

    Some(options)
}

#[async_trait]
impl Adapter for MySqlAdapter {
    fn backend(&self) -> Backend {
        Backend::MySql
    }

    fn server_version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    fn current_catalog(&self) -> Option<&str> {
        self.database.as_deref()
    }

    async fn catalogs(&self) -> Result<Vec<Catalog>> {
        // Aliased because information_schema column case moved between MySQL 5.7
        // and 8.0; the alias is what `try_get` matches on either.
        let rows = sqlx::query(
            "SELECT schema_name AS name, schema_name = DATABASE() AS is_current \
             FROM information_schema.schemata \
             WHERE schema_name NOT IN \
                 ('information_schema', 'performance_schema', 'mysql', 'sys') \
             ORDER BY schema_name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Error::query)?;

        Ok(rows
            .iter()
            .filter_map(|row| {
                Some(Catalog {
                    name: row.try_get("name").ok()?,
                    is_current: row.try_get::<i8, _>("is_current").unwrap_or(0) != 0,
                })
            })
            .collect())
    }

    async fn schemas(&self, _catalog: &str) -> Result<Vec<String>> {
        // In MySQL the database is the schema; there is no level in between.
        Ok(Vec::new())
    }

    async fn objects(&self, catalog: &str, _schema: Option<&str>) -> Result<Vec<ObjectRef>> {
        let rows = sqlx::query(
            "SELECT table_name AS name, table_type AS kind \
             FROM information_schema.tables \
             WHERE table_schema = ? \
             ORDER BY table_type, table_name",
        )
        .bind(catalog)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::query)?;

        Ok(rows
            .iter()
            .filter_map(|row| {
                let name: String = row.try_get("name").ok()?;
                let kind = match row
                    .try_get::<String, _>("kind")
                    .unwrap_or_default()
                    .as_str()
                {
                    "VIEW" => ObjectKind::View,
                    _ => ObjectKind::Table,
                };
                Some(ObjectRef::new(Some(catalog.to_string()), None, name, kind))
            })
            .collect())
    }

    async fn columns(&self, object: &ObjectRef) -> Result<Vec<Column>> {
        let catalog = object
            .catalog
            .as_deref()
            .or(self.database.as_deref())
            .unwrap_or_default();

        let rows = sqlx::query(
            "SELECT column_name AS name, column_type AS type_name, \
                    is_nullable AS nullable, column_default AS default_value, \
                    column_key AS column_key \
             FROM information_schema.columns \
             WHERE table_schema = ? AND table_name = ? \
             ORDER BY ordinal_position",
        )
        .bind(catalog)
        .bind(&object.name)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::query)?;

        Ok(rows
            .iter()
            .filter_map(|row| {
                Some(Column {
                    name: row.try_get("name").ok()?,
                    type_name: row.try_get("type_name").unwrap_or_default(),
                    nullable: row
                        .try_get::<String, _>("nullable")
                        .ok()
                        .map(|v| v.eq_ignore_ascii_case("YES")),
                    default: row
                        .try_get::<Option<String>, _>("default_value")
                        .ok()
                        .flatten(),
                    primary_key: row
                        .try_get::<String, _>("column_key")
                        .is_ok_and(|key| key == "PRI"),
                })
            })
            .collect())
    }

    async fn run(
        &self,
        statement: &Bound,
        limit: Option<usize>,
        cancel: &CancellationToken,
    ) -> Result<ResultSet> {
        self.run_on_own_connection(statement, limit, cancel).await
    }

    async fn run_transaction(
        &self,
        statements: &[Bound],
        limit: Option<usize>,
        commit: bool,
        cancel: &CancellationToken,
    ) -> Result<Vec<ResultSet>> {
        sqlx_common::run_transaction::<MySql>(&self.pool, statements, limit, commit, &CODEC, cancel)
            .await
    }

    async fn open_catalog(&self, _catalog: &str) -> Result<Option<Box<dyn Adapter>>> {
        // MySQL reads `db`.`table` across databases on one connection.
        Ok(None)
    }
}

fn affected(result: &MySqlQueryResult) -> u64 {
    result.rows_affected()
}

fn bind<'q>(
    query: Binding<'q, MySql>,
    value: &'q Value,
    _expected: Option<&MySqlTypeInfo>,
) -> std::result::Result<Binding<'q, MySql>, String> {
    Ok(bind_as_given!(query, value))
}

fn decode(row: &MySqlRow, idx: usize) -> Value {
    let type_name = column_type(row, idx);
    let unsigned = type_name.ends_with(" UNSIGNED");
    let base = type_name.trim_end_matches(" UNSIGNED");

    match base {
        "TINYINT" | "SMALLINT" | "MEDIUMINT" | "INT" | "BIGINT" | "YEAR" => {
            if unsigned {
                decode_as!(row, idx, u64, |v: u64| Value::Int(v as i64));
            }
            decode_as!(row, idx, i64, Value::Int)
        }
        "BOOLEAN" | "BOOL" => decode_as!(row, idx, bool, Value::Bool),
        "FLOAT" | "DOUBLE" => decode_as!(row, idx, f64, Value::Float),
        "DECIMAL" | "NEWDECIMAL" => {
            decode_as!(
                row,
                idx,
                rust_decimal::Decimal,
                |v: rust_decimal::Decimal| { Value::Decimal(v.to_string()) }
            )
        }
        "VARCHAR" | "CHAR" | "TEXT" | "TINYTEXT" | "MEDIUMTEXT" | "LONGTEXT" | "ENUM" | "SET" => {
            decode_as!(row, idx, String, Value::Text)
        }
        "JSON" => decode_as!(row, idx, serde_json::Value, |v: serde_json::Value| {
            Value::Json(v.to_string())
        }),
        "BINARY" | "VARBINARY" | "BLOB" | "TINYBLOB" | "MEDIUMBLOB" | "LONGBLOB" | "BIT"
        | "GEOMETRY" => decode_as!(row, idx, Vec<u8>, Value::Bytes),
        "DATE" => decode_as!(row, idx, chrono::NaiveDate, |v: chrono::NaiveDate| {
            Value::Timestamp(v.to_string())
        }),
        "TIME" => decode_as!(row, idx, chrono::NaiveTime, |v: chrono::NaiveTime| {
            Value::Timestamp(v.to_string())
        }),
        "DATETIME" | "TIMESTAMP" => {
            decode_as!(
                row,
                idx,
                chrono::NaiveDateTime,
                |v: chrono::NaiveDateTime| {
                    Value::Timestamp(v.format("%Y-%m-%d %H:%M:%S%.f").to_string())
                }
            )
        }
        _ => {}
    }

    decode_fallback!(row, idx, type_name)
}

fn column_type(row: &MySqlRow, idx: usize) -> String {
    use sqlx::{Column as _, TypeInfo as _};
    row.columns()
        .get(idx)
        .map(|c| c.type_info().name().to_ascii_uppercase())
        .unwrap_or_default()
}
