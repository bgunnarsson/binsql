use std::time::Instant;

use async_trait::async_trait;
use futures_util::TryStreamExt;
use tiberius::{AuthMethod, ColumnData, ColumnType, Config, QueryItem, Row};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};
use tokio_util::sync::CancellationToken;

use super::{Adapter, returns_rows};
use crate::backend::Backend;
use crate::error::{Error, Result};
use crate::schema::{Catalog, ObjectKind, ObjectRef};
use crate::value::{Column, ResultSet, Value};

type Connection = tiberius::Client<Compat<TcpStream>>;

pub struct MsSqlAdapter {
    /// tiberius drives one TDS connection and every call needs `&mut`, so the
    /// adapter serialises access rather than pooling. A terminal UI runs one
    /// statement at a time anyway.
    client: Mutex<Connection>,
    /// Kept so a cancelled query can be replaced with a fresh connection —
    /// see [`MsSqlAdapter::query`].
    config: Config,
    label: String,
    version: Option<String>,
    database: Option<String>,
}

impl MsSqlAdapter {
    pub async fn connect(dsn: &str) -> Result<Self> {
        let config = build_config(dsn).await?;
        let client = open(&config, dsn).await?;

        let adapter = MsSqlAdapter {
            client: Mutex::new(client),
            config,
            label: dsn.to_string(),
            version: None,
            database: None,
        };

        let version = adapter
            .scalar("SELECT @@VERSION")
            .await
            .ok()
            .flatten()
            .map(shorten_version);
        let database = adapter.scalar("SELECT DB_NAME()").await.ok().flatten();

        Ok(MsSqlAdapter {
            version,
            database,
            ..adapter
        })
    }

    /// Runs a statement expected to yield a single text cell.
    async fn scalar(&self, sql: &str) -> Result<Option<String>> {
        let result = self.ask(sql, Some(1)).await?;
        Ok(result
            .rows
            .first()
            .and_then(|row| row.first())
            .filter(|value| !value.is_null())
            .map(Value::to_text))
    }

    /// Runs an introspection query. Nothing here is worth interrupting — these
    /// are catalogue reads, not the statement someone typed — so none of them
    /// carries a cancellation token.
    async fn ask(&self, sql: &str, limit: Option<usize>) -> Result<ResultSet> {
        self.query(sql, limit, &CancellationToken::new()).await
    }

    /// Runs a row-returning statement.
    ///
    /// A cancel here costs the connection. TDS carries an attention signal for
    /// exactly this, but tiberius does not expose one, and abandoning a result
    /// stream mid-flight would leave the next statement reading the last one's
    /// rows. So the connection is dropped — which is also what tells the server
    /// to stop — and replaced before the lock is released, leaving the adapter
    /// as usable as it was before.
    async fn query(
        &self,
        sql: &str,
        limit: Option<usize>,
        cancel: &CancellationToken,
    ) -> Result<ResultSet> {
        let start = Instant::now();
        let mut client = self.client.lock().await;
        // Sending the batch is not the part worth interrupting — the waiting
        // is, and that is the loop below, which notices a token cancelled in
        // the meantime on its first turn.
        let mut stream = client.simple_query(sql).await.map_err(Error::query)?;

        let mut columns: Vec<Column> = Vec::new();
        let mut rows: Vec<Vec<Value>> = Vec::new();
        let mut truncated = false;
        let mut cancelled = false;

        loop {
            let item = tokio::select! {
                item = stream.try_next() => item.map_err(Error::query)?,
                () = cancel.cancelled() => {
                    cancelled = true;
                    break;
                }
            };
            let Some(item) = item else {
                break;
            };

            match item {
                QueryItem::Metadata(meta) => {
                    if columns.is_empty() {
                        columns = meta
                            .columns()
                            .iter()
                            .map(|c| Column::new(c.name(), type_label(c.column_type())))
                            .collect();
                    }
                }
                QueryItem::Row(row) => {
                    if columns.is_empty() {
                        columns = row
                            .columns()
                            .iter()
                            .map(|c| Column::new(c.name(), type_label(c.column_type())))
                            .collect();
                    }
                    if limit.is_some_and(|max| rows.len() >= max) {
                        truncated = true;
                        break;
                    }
                    rows.push(decode_row(&row));
                }
            }
        }

        // The stream borrows the client, so it has to go before the connection
        // underneath it can be replaced.
        drop(stream);
        if cancelled {
            return self.reconnect(&mut client).await;
        }

        Ok(ResultSet {
            columns,
            rows,
            rows_affected: None,
            elapsed: start.elapsed(),
            truncated,
        })
    }

    async fn execute(&self, sql: &str, cancel: &CancellationToken) -> Result<ResultSet> {
        let start = Instant::now();
        let mut client = self.client.lock().await;
        let finished = tokio::select! {
            result = client.execute(sql, &[]) => Some(result.map_err(Error::query)?),
            () = cancel.cancelled() => None,
        };
        let Some(result) = finished else {
            return self.reconnect(&mut client).await;
        };
        Ok(ResultSet::affected(result.total(), start.elapsed()))
    }

    /// Replaces the connection after a cancel and reports the cancel. A failure
    /// to reconnect is what the caller hears about instead, since that is the
    /// state the adapter is actually in.
    async fn reconnect(&self, client: &mut Connection) -> Result<ResultSet> {
        *client = open(&self.config, &self.label).await?;
        Err(Error::Cancelled)
    }
}

/// Opens one TDS connection. `label` names the data source in a connect error,
/// since a tiberius `Config` will not say where it came from.
async fn open(config: &Config, label: &str) -> Result<Connection> {
    let tcp = TcpStream::connect(config.get_addr())
        .await
        .map_err(|e| Error::connect(label, e))?;
    tcp.set_nodelay(true)
        .map_err(|e| Error::connect(label, e))?;

    Connection::connect(config.clone(), tcp.compat_write())
        .await
        .map_err(|e| Error::connect(label, e))
}

/// `SELECT @@VERSION` returns a multi-line banner; the header wants line one.
fn shorten_version(raw: String) -> String {
    raw.lines().next().unwrap_or_default().trim().to_string()
}

#[async_trait]
impl Adapter for MsSqlAdapter {
    fn backend(&self) -> Backend {
        Backend::MsSql
    }

    fn server_version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    fn current_catalog(&self) -> Option<&str> {
        self.database.as_deref()
    }

    async fn catalogs(&self) -> Result<Vec<Catalog>> {
        // state = 0 is ONLINE; the rest cannot be read from anyway.
        let result = self
            .ask(
                "SELECT name, CASE WHEN name = DB_NAME() THEN 1 ELSE 0 END \
                 FROM sys.databases WHERE state = 0 ORDER BY name",
                None,
            )
            .await?;

        Ok(result
            .rows
            .iter()
            .filter_map(|row| {
                Some(Catalog {
                    name: row.first()?.to_text(),
                    is_current: matches!(row.get(1), Some(Value::Int(1))),
                })
            })
            .collect())
    }

    async fn schemas(&self, catalog: &str) -> Result<Vec<String>> {
        let schemas = qualify(catalog, "sys.schemas");
        let result = self
            .ask(
                &format!(
                    "SELECT name FROM {schemas} \
                     WHERE name NOT IN ('sys', 'INFORMATION_SCHEMA', 'guest') \
                       AND name NOT LIKE 'db\\_%' ESCAPE '\\' \
                     ORDER BY name"
                ),
                None,
            )
            .await?;

        Ok(result
            .rows
            .iter()
            .filter_map(|row| row.first().map(Value::to_text))
            .collect())
    }

    async fn objects(&self, catalog: &str, schema: Option<&str>) -> Result<Vec<ObjectRef>> {
        let schema = schema.unwrap_or("dbo");
        let objects = qualify(catalog, "sys.objects");
        let schemas = qualify(catalog, "sys.schemas");
        let result = self
            .ask(
                &format!(
                    "SELECT o.name, o.type FROM {objects} o \
                     JOIN {schemas} s ON s.schema_id = o.schema_id \
                     WHERE s.name = {} AND o.type IN ('U', 'V') \
                     ORDER BY o.type, o.name",
                    quote_literal(schema)
                ),
                None,
            )
            .await?;

        Ok(result
            .rows
            .iter()
            .filter_map(|row| {
                let name = row.first()?.to_text();
                let kind = match row.get(1)?.to_text().trim() {
                    "V" => ObjectKind::View,
                    _ => ObjectKind::Table,
                };
                Some(ObjectRef::new(
                    Some(catalog.to_string()),
                    Some(schema.to_string()),
                    name,
                    kind,
                ))
            })
            .collect())
    }

    async fn columns(&self, object: &ObjectRef) -> Result<Vec<Column>> {
        let catalog = object
            .catalog
            .as_deref()
            .or(self.database.as_deref())
            .unwrap_or("master");
        let schema = object.schema.as_deref().unwrap_or("dbo");

        let columns = qualify(catalog, "sys.columns");
        let objects = qualify(catalog, "sys.objects");
        let schemas = qualify(catalog, "sys.schemas");
        let types = qualify(catalog, "sys.types");
        let index_columns = qualify(catalog, "sys.index_columns");
        let indexes = qualify(catalog, "sys.indexes");
        let defaults = qualify(catalog, "sys.default_constraints");

        let sql = format!(
            "SELECT c.name, t.name AS type_name, c.is_nullable, \
                    d.definition, \
                    CASE WHEN pk.column_id IS NULL THEN 0 ELSE 1 END AS is_pk \
             FROM {columns} c \
             JOIN {objects} o ON o.object_id = c.object_id \
             JOIN {schemas} s ON s.schema_id = o.schema_id \
             LEFT JOIN {types} t ON t.user_type_id = c.user_type_id \
             LEFT JOIN {defaults} d ON d.object_id = c.default_object_id \
             LEFT JOIN ( \
                 SELECT ic.object_id, ic.column_id FROM {index_columns} ic \
                 JOIN {indexes} i ON i.object_id = ic.object_id \
                                 AND i.index_id = ic.index_id \
                 WHERE i.is_primary_key = 1 \
             ) pk ON pk.object_id = c.object_id AND pk.column_id = c.column_id \
             WHERE s.name = {} AND o.name = {} \
             ORDER BY c.column_id",
            quote_literal(schema),
            quote_literal(&object.name),
        );

        let result = self.ask(&sql, None).await?;

        Ok(result
            .rows
            .iter()
            .filter_map(|row| {
                Some(Column {
                    name: row.first()?.to_text(),
                    type_name: row.get(1).map(Value::to_text).unwrap_or_default(),
                    nullable: row.get(2).map(|v| !matches!(v, Value::Int(0))),
                    default: row.get(3).filter(|v| !v.is_null()).map(Value::to_text),
                    primary_key: matches!(row.get(4), Some(Value::Int(1))),
                })
            })
            .collect())
    }

    async fn run(
        &self,
        sql: &str,
        limit: Option<usize>,
        cancel: &CancellationToken,
    ) -> Result<ResultSet> {
        if returns_rows(sql, Backend::MsSql) {
            self.query(sql, limit, cancel).await
        } else {
            self.execute(sql, cancel).await
        }
    }

    async fn open_catalog(&self, _catalog: &str) -> Result<Option<Box<dyn Adapter>>> {
        // SQL Server reads three-part names across databases on one connection.
        Ok(None)
    }
}

/// Prefixes a system view with its database, so one connection can introspect
/// every catalog on the server.
fn qualify(catalog: &str, view: &str) -> String {
    format!("[{}].{view}", catalog.replace(']', "]]"))
}

/// Escapes a string for inlining. Introspection has to inline schema and table
/// names because a catalog cannot be parameterised, so the literals beside them
/// are inlined the same way.
fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn type_label(column_type: ColumnType) -> String {
    let name = match column_type {
        ColumnType::Null => "null",
        ColumnType::Bit | ColumnType::Bitn => "bit",
        ColumnType::Int1 => "tinyint",
        ColumnType::Int2 => "smallint",
        ColumnType::Int4 => "int",
        ColumnType::Int8 => "bigint",
        ColumnType::Intn => "int",
        ColumnType::Float4 | ColumnType::Float8 | ColumnType::Floatn => "float",
        ColumnType::Money | ColumnType::Money4 => "money",
        ColumnType::Datetime | ColumnType::Datetime4 | ColumnType::Datetimen => "datetime",
        ColumnType::Datetime2 => "datetime2",
        ColumnType::DatetimeOffsetn => "datetimeoffset",
        ColumnType::Daten => "date",
        ColumnType::Timen => "time",
        ColumnType::Guid => "uniqueidentifier",
        ColumnType::Decimaln | ColumnType::Numericn => "decimal",
        ColumnType::BigVarBin | ColumnType::BigBinary | ColumnType::Image => "varbinary",
        ColumnType::BigVarChar | ColumnType::BigChar | ColumnType::Text => "varchar",
        ColumnType::NVarchar | ColumnType::NChar | ColumnType::NText => "nvarchar",
        ColumnType::Xml => "xml",
        ColumnType::Udt => "udt",
        ColumnType::SSVariant => "sql_variant",
    };
    name.to_string()
}

fn decode_row(row: &Row) -> Vec<Value> {
    (0..row.len())
        .map(|idx| {
            let data = row.cells().nth(idx).map(|(_, data)| data);
            match data {
                Some(data) => decode(row, idx, data),
                None => Value::Null,
            }
        })
        .collect()
}

fn decode(row: &Row, idx: usize, data: &ColumnData<'static>) -> Value {
    match data {
        ColumnData::U8(v) => opt(v.map(|v| Value::Int(i64::from(v)))),
        ColumnData::I16(v) => opt(v.map(|v| Value::Int(i64::from(v)))),
        ColumnData::I32(v) => opt(v.map(|v| Value::Int(i64::from(v)))),
        ColumnData::I64(v) => opt(v.map(Value::Int)),
        ColumnData::F32(v) => opt(v.map(|v| Value::Float(f64::from(v)))),
        ColumnData::F64(v) => opt(v.map(Value::Float)),
        ColumnData::Bit(v) => opt(v.map(Value::Bool)),
        ColumnData::String(v) => opt(v.as_ref().map(|v| Value::Text(v.to_string()))),
        ColumnData::Guid(v) => opt(v.map(|v| Value::Uuid(v.to_string()))),
        ColumnData::Binary(v) => opt(v.as_ref().map(|v| Value::Bytes(v.to_vec()))),
        ColumnData::Numeric(v) => opt(v.map(|v| Value::Decimal(v.to_string()))),
        ColumnData::Xml(v) => opt(v
            .as_ref()
            .map(|v| Value::Text(v.clone().into_owned().into_string()))),

        // The temporal variants hold TDS-internal representations, so they go
        // back through `try_get`, which owns the conversion to chrono.
        ColumnData::DateTime(_) | ColumnData::SmallDateTime(_) | ColumnData::DateTime2(_) => {
            timestamp(row.try_get::<chrono::NaiveDateTime, _>(idx), |v| {
                v.format("%Y-%m-%d %H:%M:%S%.f").to_string()
            })
        }
        ColumnData::Date(_) => {
            timestamp(row.try_get::<chrono::NaiveDate, _>(idx), |v| v.to_string())
        }
        ColumnData::Time(_) => {
            timestamp(row.try_get::<chrono::NaiveTime, _>(idx), |v| v.to_string())
        }
        ColumnData::DateTimeOffset(_) => {
            timestamp(row.try_get::<chrono::DateTime<chrono::Utc>, _>(idx), |v| {
                v.format("%Y-%m-%d %H:%M:%S%.f%:z").to_string()
            })
        }
    }
}

fn opt(value: Option<Value>) -> Value {
    value.unwrap_or(Value::Null)
}

fn timestamp<T>(result: tiberius::Result<Option<T>>, format: impl Fn(T) -> String) -> Value {
    match result {
        Ok(Some(value)) => Value::Timestamp(format(value)),
        _ => Value::Null,
    }
}

/// Builds a tiberius config from the connection-string forms binsql accepts.
///
/// `fedauth=` is the marker the v2 SQL Server adapter used to switch to the
/// Azure AD driver, and it keeps that meaning here: it selects a token obtained
/// from the Azure CLI rather than a password in the string.
async fn build_config(dsn: &str) -> Result<Config> {
    let (connection_string, federated) = split_fedauth(dsn);
    let ado = if connection_string.contains("://") {
        url_to_ado(&connection_string)?
    } else {
        connection_string
    };

    let mut config = Config::from_ado_string(&ado).map_err(|e| Error::connect(dsn, e))?;
    if federated {
        config.authentication(AuthMethod::aad_token(azure_cli_token().await?));
    }
    Ok(config)
}

/// Removes the `fedauth=` parameter tiberius does not understand, reporting
/// whether it was there.
fn split_fedauth(dsn: &str) -> (String, bool) {
    let mut federated = false;
    let kept: Vec<&str> = dsn
        .split(';')
        .filter(|part| {
            if part.trim().to_ascii_lowercase().starts_with("fedauth=") {
                federated = true;
                false
            } else {
                true
            }
        })
        .collect();
    (kept.join(";"), federated)
}

/// `sqlserver://user:pass@host:port?database=db` — the go-mssqldb URL form that
/// v2 profiles use. tiberius only parses ADO and JDBC strings, so it is
/// rewritten rather than rejected.
fn url_to_ado(dsn: &str) -> Result<String> {
    let rest = dsn
        .split_once("://")
        .map(|(_, rest)| rest)
        .ok_or_else(|| Error::UnknownBackend(dsn.to_string()))?;

    let (credentials, host_part) = match rest.rfind('@') {
        Some(at) => (Some(&rest[..at]), &rest[at + 1..]),
        None => (None, rest),
    };

    let (authority, query) = match host_part.split_once('?') {
        Some((authority, query)) => (authority, Some(query)),
        None => (host_part, None),
    };
    let authority = authority.trim_end_matches('/');

    let mut parts = Vec::new();
    let server = match authority.rsplit_once(':') {
        Some((host, port)) => format!("tcp:{host},{port}"),
        None => format!("tcp:{authority}"),
    };
    parts.push(format!("server={server}"));

    if let Some(credentials) = credentials {
        let (user, password) = match credentials.split_once(':') {
            Some((user, password)) => (user, Some(password)),
            None => (credentials, None),
        };
        parts.push(format!("user id={}", decode_percent(user)));
        if let Some(password) = password {
            parts.push(format!("password={}", decode_percent(password)));
        }
    }

    for pair in query
        .unwrap_or_default()
        .split('&')
        .filter(|p| !p.is_empty())
    {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        let value = decode_percent(value);
        match key.to_ascii_lowercase().as_str() {
            "database" => parts.push(format!("database={value}")),
            "encrypt" => parts.push(format!("encrypt={value}")),
            "trustservercertificate" => parts.push(format!("trustservercertificate={value}")),
            "app name" | "application name" => parts.push(format!("application name={value}")),
            _ => {}
        }
    }

    Ok(parts.join(";"))
}

/// Minimal percent-decoding — passwords in a URL DSN routinely carry `%40`.
fn decode_percent(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Asks the Azure CLI for a SQL access token.
///
/// Shelling out to `az` is what the v2 README already told people to set up
/// (`az login`), and it keeps binsql from carrying an Azure identity stack for
/// the one auth path that needs it.
async fn azure_cli_token() -> Result<String> {
    let output = tokio::process::Command::new("az")
        .args([
            "account",
            "get-access-token",
            "--resource",
            "https://database.windows.net/",
            "--output",
            "json",
        ])
        // The Azure CLI's Python warnings go to stderr and have broken token
        // reads before; silencing them is the documented workaround.
        .env("PYTHONWARNINGS", "ignore")
        .output()
        .await
        .map_err(|e| {
            Error::connect(
                "azure ad",
                anyhow::anyhow!("running `az`: {e}. Is the Azure CLI installed?"),
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::connect(
            "azure ad",
            anyhow::anyhow!("`az account get-access-token` failed: {}", stderr.trim()),
        ));
    }

    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| Error::connect("azure ad", anyhow::anyhow!("parsing `az` output: {e}")))?;

    parsed
        .get("accessToken")
        .and_then(|token| token.as_str())
        .map(str::to_string)
        .ok_or_else(|| {
            Error::connect(
                "azure ad",
                anyhow::anyhow!("`az` returned no accessToken. Try `az login`."),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_fedauth_and_reports_it() {
        let (rest, federated) =
            split_fedauth("server=tcp:h,1433;fedauth=ActiveDirectoryDefault;database=db");
        assert!(federated);
        assert_eq!(rest, "server=tcp:h,1433;database=db");

        let (rest, federated) = split_fedauth("server=tcp:h,1433");
        assert!(!federated);
        assert_eq!(rest, "server=tcp:h,1433");
    }

    #[test]
    fn rewrites_go_url_dsn() {
        let ado = url_to_ado("sqlserver://sa:p%40ss@localhost:1433?database=app&encrypt=true")
            .expect("valid dsn");
        assert_eq!(
            ado,
            "server=tcp:localhost,1433;user id=sa;password=p@ss;database=app;encrypt=true"
        );
    }
}
