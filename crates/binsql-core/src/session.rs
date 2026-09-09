use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::adapter::{self, Adapter, is_mutating};
use crate::backend::{Backend, Dialect};
use crate::config::DataSource;
use crate::error::{Error, Result};
use crate::schema::ObjectRef;
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
        let name = name.into();
        let primary: Arc<dyn Adapter> =
            Arc::from(adapter::connect(source.backend, &source.dsn).await?);
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
        if self.source.read_only && is_mutating(sql) {
            return Err(Error::ReadOnly {
                data_source: self.name.clone(),
                statement: first_word(sql),
            });
        }
        self.adapter_for(catalog).await?.run(sql, limit).await
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

fn first_word(sql: &str) -> String {
    sql.split_whitespace()
        .next()
        .unwrap_or("statement")
        .to_ascii_uppercase()
}
