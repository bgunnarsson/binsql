use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::adapter::{self, Adapter};
use crate::backend::{Backend, Dialect};
use crate::config::{DataSource, mask_dsn};
use crate::error::{Error, Reason, Result};
use crate::schema::ObjectRef;
use crate::secrets::Resolver;
use crate::sql::{self, Bound};
use crate::value::{Column, ResultSet};

/// Where trying a data source stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Reading the connection string out of the vault or credential store.
    Secret,
    /// Getting an Azure AD token for the server.
    Token,
    /// Reaching the server and logging in.
    Connect,
}

impl Stage {
    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Secret => "secret",
            Stage::Token => "token",
            Stage::Connect => "connect",
        }
    }
}

/// Why trying a data source failed, with the connection string masked out of
/// the message.
#[derive(Debug)]
pub struct ProbeFailure {
    pub stage: Stage,
    pub message: String,
}

/// `message` with every copy of `dsn` replaced by its masked form, and then
/// whatever masking hid — the password, when there is one — wherever else the
/// message quotes it, as in a host the URL's userinfo is still attached to.
pub fn masked(mut message: String, backend: Backend, dsn: &str) -> String {
    let mask = mask_dsn(backend, dsn);
    for raw in [dsn, dsn.trim()] {
        if !raw.is_empty() {
            message = message.replace(raw, &mask);
        }
    }
    let hidden = hidden(dsn, &mask);
    if !hidden.is_empty() {
        message = message.replace(hidden, "****");
    }
    message
}

/// The span of `dsn` that `mask` differs in.
fn hidden<'a>(dsn: &'a str, mask: &str) -> &'a str {
    let start = dsn
        .char_indices()
        .zip(mask.chars())
        .find(|((_, a), b)| a != b)
        .map_or(dsn.len().min(mask.len()), |((i, _), _)| i);
    let end = dsn[start..]
        .chars()
        .rev()
        .zip(mask[start.min(mask.len())..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum::<usize>();
    &dsn[start..dsn.len() - end]
}

/// One open data source.
///
/// A session is usually one connection, but a backend that cannot read across
/// its own databases grows one per catalog on demand — which is the difference
/// between "binsql is connected to a database" and "binsql is connected to a
/// server", and the reason more than one data source can be open at once.
pub struct Session {
    pub name: String,
    pub source: DataSource,
    primary: Arc<dyn Adapter>,
    per_catalog: RwLock<HashMap<String, Arc<dyn Adapter>>>,
}

impl Session {
    pub async fn open(name: impl Into<String>, source: DataSource) -> Result<Session> {
        Session::open_with(name, source, &Resolver::from_env()).await
    }

    /// Opens against a specific resolver. Tests use this to supply a cache
    /// directory rather than the real one.
    pub async fn open_with(
        name: impl Into<String>,
        source: DataSource,
        resolver: &Resolver,
    ) -> Result<Session> {
        let name = name.into();
        // The stored DSN may name a secret rather than hold one, so it is
        // resolved before anything tries to parse it as a connection string.
        let dsn = resolver.resolve(&source.dsn).await?;
        let primary: Arc<dyn Adapter> = Arc::from(adapter::connect(source.backend, &dsn).await?);
        Ok(Session {
            name,
            source,
            primary,
            per_catalog: RwLock::new(HashMap::new()),
        })
    }

    /// Resolves the source's connection string and connects, then lets the
    /// connection go, saying which stage stopped it when one did. `fresh`
    /// skips the cached secret.
    pub async fn probe(
        source: &DataSource,
        resolver: &Resolver,
        fresh: bool,
    ) -> std::result::Result<(), ProbeFailure> {
        let resolved = if fresh {
            resolver.resolve_fresh(&source.dsn).await
        } else {
            resolver.resolve(&source.dsn).await
        };
        // A resolver or driver may quote the connection string back, password
        // and all, so it is swapped for its masked form.
        let dsn = resolved.map_err(|err| ProbeFailure {
            stage: Stage::Secret,
            message: masked(err.to_string(), source.backend, &source.dsn),
        })?;
        match adapter::connect(source.backend, &dsn).await {
            Ok(_) => Ok(()),
            Err(err) => {
                let stage = match &err {
                    Error::Connect {
                        reason: Some(Reason::AzureAdToken),
                        ..
                    } => Stage::Token,
                    _ => Stage::Connect,
                };
                Err(ProbeFailure {
                    stage,
                    message: masked(err.to_string(), source.backend, &dsn),
                })
            }
        }
    }

    pub fn backend(&self) -> Backend {
        self.primary.backend()
    }

    pub fn dialect(&self) -> Dialect {
        self.primary.backend().dialect()
    }

    pub fn server_version(&self) -> Option<&str> {
        self.primary.server_version()
    }

    pub fn current_catalog(&self) -> Option<&str> {
        self.primary.current_catalog()
    }

    pub fn is_read_only(&self) -> bool {
        self.source.read_only
    }

    /// The adapter that can see `catalog`, opening a second connection only
    /// when the backend needs one.
    pub async fn adapter_for(&self, catalog: Option<&str>) -> Result<Arc<dyn Adapter>> {
        let Some(catalog) = catalog else {
            return Ok(Arc::clone(&self.primary));
        };
        if self.primary.current_catalog() == Some(catalog)
            || self.primary.backend().catalogs_share_connection()
        {
            return Ok(Arc::clone(&self.primary));
        }

        if let Some(existing) = self.per_catalog.read().await.get(catalog) {
            return Ok(Arc::clone(existing));
        }

        let Some(opened) = self.primary.open_catalog(catalog).await? else {
            return Ok(Arc::clone(&self.primary));
        };
        let opened: Arc<dyn Adapter> = Arc::from(opened);

        let mut cache = self.per_catalog.write().await;
        // Another task may have opened the same catalog while this one was
        // connecting; keep whichever landed first so the map stays canonical.
        let stored = cache
            .entry(catalog.to_string())
            .or_insert_with(|| Arc::clone(&opened));
        Ok(Arc::clone(stored))
    }

    /// Runs a statement, refusing writes on a read-only data source before
    /// anything reaches the server.
    pub async fn run(
        &self,
        catalog: Option<&str>,
        sql: &str,
        limit: Option<usize>,
    ) -> Result<ResultSet> {
        self.run_cancellable(catalog, sql, limit, &CancellationToken::new())
            .await
    }

    /// The same, with a way out. Cancelling the token abandons the query and
    /// yields [`Error::Cancelled`]; the adapter is left able to run the next
    /// statement, and stops the server where its backend gives it the means.
    pub async fn run_cancellable(
        &self,
        catalog: Option<&str>,
        sql: &str,
        limit: Option<usize>,
        cancel: &CancellationToken,
    ) -> Result<ResultSet> {
        self.run_bound(catalog, &Bound::plain(sql), limit, cancel)
            .await
    }

    /// The same, for a statement with values to send beside it — see
    /// [`sql::bind`], which is what put its placeholders into this backend's
    /// spelling. Binding changes what is sent, never what the statement does,
    /// so the read-only guard reads it exactly as it reads any other.
    pub async fn run_bound(
        &self,
        catalog: Option<&str>,
        statement: &Bound,
        limit: Option<usize>,
        cancel: &CancellationToken,
    ) -> Result<ResultSet> {
        self.guard_read_only(&statement.sql)?;
        self.adapter_for(catalog)
            .await?
            .run(statement, limit, cancel)
            .await
    }

    /// Asks for the estimated plan of one read without running it. Anything
    /// else — a write, a `SHOW`, a second statement after the first — is
    /// refused before a connection is chosen: a prefix in front of a script
    /// would plan its first statement and run the rest.
    pub async fn plan(
        &self,
        catalog: Option<&str>,
        sql: &str,
        limit: Option<usize>,
        cancel: &CancellationToken,
    ) -> Result<ResultSet> {
        let backend = self.backend();
        let mut statements = sql::split(sql, backend);
        let statement = match statements.pop() {
            Some(statement) if statements.is_empty() && sql::plannable(&statement.sql, backend) => {
                statement
            }
            _ => {
                return Err(Error::NotPlannable {
                    statement: sql::summarize(sql, backend, 80),
                });
            }
        };
        self.guard_read_only(&statement.sql)?;
        self.adapter_for(catalog)
            .await?
            .plan(&Bound::plain(statement.sql), limit, cancel)
            .await
    }

    /// Runs several statements as one transaction, committing at the end — or
    /// rolling back, which is what makes a dry run a dry run: the statements
    /// really execute, and then nothing is kept.
    ///
    /// The read-only guard sees the whole batch before any of it is sent, so a
    /// script with one write in its middle never starts.
    pub async fn run_transaction(
        &self,
        catalog: Option<&str>,
        statements: &[Bound],
        limit: Option<usize>,
        commit: bool,
        cancel: &CancellationToken,
    ) -> Result<Vec<ResultSet>> {
        for statement in statements {
            self.guard_read_only(&statement.sql)?;
        }
        self.adapter_for(catalog)
            .await?
            .run_transaction(statements, limit, commit, cancel)
            .await
    }

    /// Refuses the whole script if any statement in it writes.
    ///
    /// Everything a script can be is checked, not just its first word: a
    /// comment above the statement, a second statement after a harmless first
    /// one, and a `WITH` fronting a `DELETE` all used to read as a `SELECT`.
    /// This runs before a connection is even chosen, so a refused script
    /// reaches no server at all.
    pub fn guard_read_only(&self, sql: &str) -> Result<()> {
        if !self.source.read_only {
            return Ok(());
        }

        let backend = self.backend();
        let offender = sql::split(sql, backend)
            .into_iter()
            .find(|statement| statement.kind.mutates());

        match offender {
            Some(statement) => Err(Error::ReadOnly {
                data_source: self.name.clone(),
                statement: sql::summarize(&statement.sql, backend, 80),
            }),
            None => Ok(()),
        }
    }

    pub async fn catalogs(&self) -> Result<Vec<crate::schema::Catalog>> {
        self.primary.catalogs().await
    }

    pub async fn schemas(&self, catalog: &str) -> Result<Vec<String>> {
        self.adapter_for(Some(catalog))
            .await?
            .schemas(catalog)
            .await
    }

    pub async fn objects(&self, catalog: &str, schema: Option<&str>) -> Result<Vec<ObjectRef>> {
        self.adapter_for(Some(catalog))
            .await?
            .objects(catalog, schema)
            .await
    }

    pub async fn columns(&self, object: &ObjectRef) -> Result<Vec<Column>> {
        self.adapter_for(object.catalog.as_deref())
            .await?
            .columns(object)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_password_is_masked_wherever_the_message_quotes_it() {
        let dsn = "https://sa:hunter2@kv.vault.azure.net/";
        let message = format!("{dsn} names the vault sa:hunter2@kv.vault.azure.net");
        assert_eq!(
            masked(message, Backend::MsSql, dsn),
            "https://sa:****@kv.vault.azure.net/ names the vault sa:****@kv.vault.azure.net"
        );
    }

    #[test]
    fn a_dsn_with_no_password_leaves_the_message_alone() {
        let dsn = "postgres://app@db/app";
        assert_eq!(
            masked(
                format!("connecting to {dsn}: refused"),
                Backend::Postgres,
                dsn
            ),
            "connecting to postgres://app@db/app: refused"
        );
    }
}
