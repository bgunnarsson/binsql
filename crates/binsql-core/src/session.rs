use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::adapter::{self, Adapter};
use crate::backend::{Backend, Dialect};
use crate::config::DataSource;
use crate::error::{Error, Result};
use crate::schema::ObjectRef;
use crate::secrets::Resolver;
use crate::sql;
use crate::value::{Column, ResultSet};

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
        self.guard_read_only(sql)?;
        self.adapter_for(catalog).await?.run(sql, limit).await
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
