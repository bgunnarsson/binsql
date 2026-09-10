use std::str::FromStr;

use async_trait::async_trait;
use sqlx::encode::{Encode, IsNull};
use sqlx::error::BoxDynError;
use sqlx::postgres::{
    PgArgumentBuffer, PgConnectOptions, PgPool, PgPoolOptions, PgQueryResult, PgRow, PgSslMode,
    PgTypeInfo, Postgres,
};
use sqlx::{Row, Type, TypeInfo as _};
use tokio_util::sync::CancellationToken;

use super::Adapter;
use super::sqlx_common::{self, Binding, Codec, decode_as, decode_fallback};
use crate::backend::Backend;
use crate::error::{Error, Result};
use crate::schema::{Catalog, ObjectKind, ObjectRef};
use crate::sql::Bound;
use crate::value::{Column, ResultSet, Value};

const CODEC: Codec<Postgres> = Codec {
    decode,
    affected,
    bind,
    describe: true,
};

pub struct PostgresAdapter {
    pool: PgPool,
    /// Kept so a sibling database can be opened without re-parsing the DSN —
    /// Postgres cannot read across databases on one connection.
    options: PgConnectOptions,
    version: Option<String>,
    database: Option<String>,
}

impl PostgresAdapter {
    pub async fn connect(dsn: &str) -> Result<Self> {
        let options = connect_options(dsn)?;
        Self::connect_with(options, dsn).await
    }

    async fn connect_with(options: PgConnectOptions, label: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .connect_with(options.clone())
            .await
            .map_err(|e| Error::connect(label, e))?;

        let version: Option<String> = sqlx::query_scalar("SELECT version()")
            .fetch_one(&pool)
            .await
            .ok();
        let database: Option<String> = sqlx::query_scalar("SELECT current_database()")
            .fetch_one(&pool)
            .await
            .ok();

        Ok(PostgresAdapter {
            pool,
            options,
            version: version.map(shorten_version),
            database,
        })
    }

    /// Runs the statement on a connection of its own so that a cancel has
    /// something to name. Postgres stops a running query only when a *second*
    /// connection asks it to, by the first one's backend pid — closing the
    /// socket does not do it, since a backend busy in a long scan is not
    /// listening for the client to go away.
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
        let pid: Option<i32> = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *connection)
            .await
            .ok();

        let result =
            sqlx_common::run::<Postgres>(&mut connection, statement, limit, &CODEC, cancel).await;

        if matches!(result, Err(Error::Cancelled))
            && let Some(pid) = pid
        {
            // Best-effort by Postgres's own definition: `pg_cancel_backend`
            // asks the backend to stop at its next opportunity rather than
            // making it. Sent while the connection above is still checked out,
            // so the pool is not left draining a query nobody is waiting for.
            let _ = sqlx::query("SELECT pg_cancel_backend($1)")
                .bind(pid)
                .execute(&self.pool)
                .await;
        }

        result
    }
}

/// `postgres://…` goes straight to sqlx; the libpq keyword form does not, so it
/// is translated here rather than rejected — v2 profiles are full of it.
fn connect_options(dsn: &str) -> Result<PgConnectOptions> {
    let trimmed = dsn.trim();
    if trimmed.contains("://") {
        return PgConnectOptions::from_str(trimmed).map_err(|e| Error::connect(dsn, e));
    }

    let mut options = PgConnectOptions::new();
    for pair in trimmed.split_whitespace() {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        options = match key.to_ascii_lowercase().as_str() {
            "host" => options.host(value),
            "port" => match value.parse() {
                Ok(port) => options.port(port),
                Err(_) => options,
            },
            "dbname" | "database" => options.database(value),
            "user" | "username" => options.username(value),
            "password" => options.password(value),
            "application_name" => options.application_name(value),
            "sslmode" => match PgSslMode::from_str(value) {
                Ok(mode) => options.ssl_mode(mode),
                Err(_) => options,
            },
            _ => options,
        };
    }
    Ok(options)
}

/// `SELECT version()` returns a paragraph. The header wants the first clause.
fn shorten_version(raw: String) -> String {
    raw.split(" on ").next().unwrap_or(&raw).trim().to_string()
}

#[async_trait]
impl Adapter for PostgresAdapter {
    fn backend(&self) -> Backend {
        Backend::Postgres
    }

    fn server_version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    fn current_catalog(&self) -> Option<&str> {
        self.database.as_deref()
    }

    async fn catalogs(&self) -> Result<Vec<Catalog>> {
        let rows = sqlx::query(
            "SELECT datname, datname = current_database() AS is_current \
             FROM pg_database \
             WHERE datallowconn AND NOT datistemplate \
             ORDER BY datname",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Error::query)?;

        Ok(rows
            .iter()
            .filter_map(|row| {
                Some(Catalog {
                    name: row.try_get("datname").ok()?,
                    is_current: row.try_get("is_current").unwrap_or(false),
                })
            })
            .collect())
    }

    async fn schemas(&self, _catalog: &str) -> Result<Vec<String>> {
        let rows = sqlx::query(
            "SELECT nspname FROM pg_namespace \
             WHERE nspname NOT LIKE 'pg\\_%' AND nspname <> 'information_schema' \
             ORDER BY nspname",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Error::query)?;

        Ok(rows
            .iter()
            .filter_map(|row| row.try_get("nspname").ok())
            .collect())
    }

    async fn objects(&self, _catalog: &str, schema: Option<&str>) -> Result<Vec<ObjectRef>> {
        let schema = schema.unwrap_or("public");
        let rows = sqlx::query(
            "SELECT c.relname, c.relkind \
             FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = $1 AND c.relkind IN ('r', 'p', 'v', 'm', 'f') \
             ORDER BY c.relkind, c.relname",
        )
        .bind(schema)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::query)?;

        Ok(rows
            .iter()
            .filter_map(|row| {
                let name: String = row.try_get("relname").ok()?;
                let relkind: i8 = row.try_get("relkind").ok()?;
                let kind = match relkind as u8 as char {
                    'v' | 'm' => ObjectKind::View,
                    _ => ObjectKind::Table,
                };
                Some(ObjectRef::new(
                    self.database.clone(),
                    Some(schema.to_string()),
                    name,
                    kind,
                ))
            })
            .collect())
    }

    async fn columns(&self, object: &ObjectRef) -> Result<Vec<Column>> {
        let rows = sqlx::query(
            "SELECT a.attname, \
                    format_type(a.atttypid, a.atttypmod) AS type_name, \
                    NOT a.attnotnull AS nullable, \
                    pg_get_expr(d.adbin, d.adrelid) AS default_value, \
                    COALESCE(i.indisprimary, false) AS primary_key \
             FROM pg_attribute a \
             JOIN pg_class c ON c.oid = a.attrelid \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum \
             LEFT JOIN pg_index i ON i.indrelid = c.oid AND i.indisprimary \
                    AND a.attnum = ANY(i.indkey) \
             WHERE n.nspname = $1 AND c.relname = $2 \
                    AND a.attnum > 0 AND NOT a.attisdropped \
             ORDER BY a.attnum",
        )
        .bind(object.schema.as_deref().unwrap_or("public"))
        .bind(&object.name)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::query)?;

        Ok(rows
            .iter()
            .filter_map(|row| {
                Some(Column {
                    name: row.try_get("attname").ok()?,
                    type_name: row.try_get("type_name").unwrap_or_default(),
                    nullable: row.try_get("nullable").ok(),
                    default: row.try_get("default_value").ok().flatten(),
                    primary_key: row.try_get("primary_key").unwrap_or(false),
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
        sqlx_common::run_transaction::<Postgres>(
            &self.pool, statements, limit, commit, &CODEC, cancel,
        )
        .await
    }

    async fn open_catalog(&self, catalog: &str) -> Result<Option<Box<dyn Adapter>>> {
        if self.database.as_deref() == Some(catalog) {
            return Ok(None);
        }
        let options = self.options.clone().database(catalog);
        let adapter = PostgresAdapter::connect_with(options, catalog).await?;
        Ok(Some(Box::new(adapter)))
    }
}

fn affected(result: &PgQueryResult) -> u64 {
    result.rows_affected()
}

/// Postgres types a parameter by the value sent for it rather than converting
/// it to what the column holds, so text sent for a date column is a type error
/// rather than a date. Each value is converted here to the type the server
/// reported for its placeholder instead — what v2's driver did, and what
/// someone typing `--arg 2024-01-01` means.
fn bind<'q>(
    query: Binding<'q, Postgres>,
    value: &'q Value,
    expected: Option<&PgTypeInfo>,
) -> std::result::Result<Binding<'q, Postgres>, String> {
    let Some(expected) = expected else {
        return Err("the server did not say what this placeholder takes".into());
    };
    if value.is_null() {
        return Ok(query.bind(AsText {
            text: None,
            type_info: expected.clone(),
        }));
    }

    let text = value.to_text();
    let bound = match expected.name() {
        "BOOL" => query.bind(parse_bool(&text).ok_or_else(|| expected_a("a boolean", &text))?),
        "INT2" => query.bind(
            text.parse::<i16>()
                .map_err(|_| expected_a("a smallint", &text))?,
        ),
        "INT4" => query.bind(
            text.parse::<i32>()
                .map_err(|_| expected_a("an integer", &text))?,
        ),
        "INT8" => query.bind(
            text.parse::<i64>()
                .map_err(|_| expected_a("a bigint", &text))?,
        ),
        "FLOAT4" => query.bind(
            text.parse::<f32>()
                .map_err(|_| expected_a("a number", &text))?,
        ),
        "FLOAT8" => query.bind(
            text.parse::<f64>()
                .map_err(|_| expected_a("a number", &text))?,
        ),
        "NUMERIC" => query.bind(
            rust_decimal::Decimal::from_str(&text)
                .or_else(|_| rust_decimal::Decimal::from_scientific(&text))
                .map_err(|_| expected_a("a number", &text))?,
        ),
        "UUID" => {
            query.bind(uuid::Uuid::parse_str(&text).map_err(|_| expected_a("a UUID", &text))?)
        }
        "JSONB" => query.bind(
            serde_json::from_str::<serde_json::Value>(&text)
                .map_err(|_| expected_a("JSON", &text))?,
        ),
        "DATE" | "TIME" | "TIMESTAMP" | "TIMESTAMPTZ" => bind_temporal(query, &text)?,
        "BYTEA" => match value {
            Value::Bytes(bytes) => query.bind(bytes.as_slice()),
            _ => query.bind(text.into_bytes()),
        },
        _ => query.bind(AsText {
            text: Some(text),
            type_info: expected.clone(),
        }),
    };
    Ok(bound)
}

/// A date or a time, as whichever of the four it reads as. Postgres casts
/// between date, timestamp and timestamptz itself, and a value written without
/// an offset is sent without one, so the server reads it in the session's own
/// time zone — exactly as it would have read the text.
fn bind_temporal<'q>(
    query: Binding<'q, Postgres>,
    text: &str,
) -> std::result::Result<Binding<'q, Postgres>, String> {
    if let Ok(zoned) = chrono::DateTime::parse_from_rfc3339(text)
        .or_else(|_| chrono::DateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S%.f%#z"))
    {
        return Ok(query.bind(zoned));
    }
    for format in [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M",
    ] {
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(text, format) {
            return Ok(query.bind(naive));
        }
    }
    if let Ok(date) = chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d") {
        return Ok(query.bind(date));
    }
    for format in ["%H:%M:%S%.f", "%H:%M"] {
        if let Ok(time) = chrono::NaiveTime::parse_from_str(text, format) {
            return Ok(query.bind(time));
        }
    }
    Err(expected_a("a date or a time", text))
}

/// Postgres's own spellings of a boolean, so what it would accept as text is
/// accepted here too.
fn parse_bool(text: &str) -> Option<bool> {
    match text.to_ascii_lowercase().as_str() {
        "t" | "true" | "y" | "yes" | "on" | "1" => Some(true),
        "f" | "false" | "n" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

fn expected_a(what: &str, text: &str) -> String {
    format!("expected {what}, got {text:?}")
}

/// A value sent as its own text under the type the server asked for. Right for
/// every type whose binary form is its text — the character types, `json`,
/// enums, and domains over any of them — and for a NULL of any type at all,
/// which sends no bytes. The server refuses anything else with a message of
/// its own.
struct AsText {
    text: Option<String>,
    type_info: PgTypeInfo,
}

impl Type<Postgres> for AsText {
    // Never consulted: `produces` always answers.
    fn type_info() -> PgTypeInfo {
        <String as Type<Postgres>>::type_info()
    }
}

impl Encode<'_, Postgres> for AsText {
    fn encode_by_ref(
        &self,
        buf: &mut PgArgumentBuffer,
    ) -> std::result::Result<IsNull, BoxDynError> {
        match &self.text {
            Some(text) => {
                buf.extend(text.as_bytes());
                Ok(IsNull::No)
            }
            None => Ok(IsNull::Yes),
        }
    }

    fn produces(&self) -> Option<PgTypeInfo> {
        Some(self.type_info.clone())
    }
}

fn decode(row: &PgRow, idx: usize) -> Value {
    let type_name = column_type(row, idx);

    match type_name.as_str() {
        "BOOL" => decode_as!(row, idx, bool, Value::Bool),
        "INT2" => decode_as!(row, idx, i16, |v| Value::Int(i64::from(v))),
        "INT4" => decode_as!(row, idx, i32, |v| Value::Int(i64::from(v))),
        "INT8" => decode_as!(row, idx, i64, Value::Int),
        "FLOAT4" => decode_as!(row, idx, f32, |v| Value::Float(f64::from(v))),
        "FLOAT8" => decode_as!(row, idx, f64, Value::Float),
        "NUMERIC" => decode_as!(
            row,
            idx,
            rust_decimal::Decimal,
            |v: rust_decimal::Decimal| { Value::Decimal(v.to_string()) }
        ),
        "TEXT" | "VARCHAR" | "BPCHAR" | "CHAR" | "NAME" | "CITEXT" => {
            decode_as!(row, idx, String, Value::Text)
        }
        "UUID" => decode_as!(row, idx, uuid::Uuid, |v: uuid::Uuid| Value::Uuid(
            v.to_string()
        )),
        "JSON" | "JSONB" => decode_as!(row, idx, serde_json::Value, |v: serde_json::Value| {
            Value::Json(v.to_string())
        }),
        "BYTEA" => decode_as!(row, idx, Vec<u8>, Value::Bytes),
        "DATE" => decode_as!(row, idx, chrono::NaiveDate, |v: chrono::NaiveDate| {
            Value::Timestamp(v.to_string())
        }),
        "TIME" => decode_as!(row, idx, chrono::NaiveTime, |v: chrono::NaiveTime| {
            Value::Timestamp(v.to_string())
        }),
        "TIMESTAMP" => decode_as!(
            row,
            idx,
            chrono::NaiveDateTime,
            |v: chrono::NaiveDateTime| {
                Value::Timestamp(v.format("%Y-%m-%d %H:%M:%S%.f").to_string())
            }
        ),
        "TIMESTAMPTZ" => {
            decode_as!(
                row,
                idx,
                chrono::DateTime<chrono::Utc>,
                |v: chrono::DateTime<chrono::Utc>| {
                    Value::Timestamp(v.format("%Y-%m-%d %H:%M:%S%.f%:z").to_string())
                }
            )
        }
        _ if type_name.ends_with("[]") => return decode_array(row, idx, &type_name),
        _ => {}
    }

    decode_fallback!(row, idx, type_name)
}

/// Arrays are common enough in Postgres that `<TEXT[]>` in a grid would be a
/// visible hole. Only the element types worth having are listed; anything else
/// still falls through to the placeholder.
fn decode_array(row: &PgRow, idx: usize, type_name: &str) -> Value {
    fn joined<T: ToString>(values: Vec<T>) -> Value {
        Value::Text(format!(
            "{{{}}}",
            values
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",")
        ))
    }

    match type_name {
        "TEXT[]" | "VARCHAR[]" | "NAME[]" => decode_as!(row, idx, Vec<String>, joined),
        "INT2[]" => decode_as!(row, idx, Vec<i16>, joined),
        "INT4[]" => decode_as!(row, idx, Vec<i32>, joined),
        "INT8[]" => decode_as!(row, idx, Vec<i64>, joined),
        "FLOAT4[]" => decode_as!(row, idx, Vec<f32>, joined),
        "FLOAT8[]" => decode_as!(row, idx, Vec<f64>, joined),
        "BOOL[]" => decode_as!(row, idx, Vec<bool>, joined),
        _ => {}
    }

    Value::Text(format!("<{type_name}>"))
}

fn column_type(row: &PgRow, idx: usize) -> String {
    use sqlx::{Column as _, TypeInfo as _};
    row.columns()
        .get(idx)
        .map(|c| c.type_info().name().to_ascii_uppercase())
        .unwrap_or_default()
}
