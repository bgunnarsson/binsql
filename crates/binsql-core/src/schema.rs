use crate::backend::Dialect;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ObjectKind {
    Table,
    View,
}

impl ObjectKind {
    pub fn label(self) -> &'static str {
        match self {
            ObjectKind::Table => "Tables",
            ObjectKind::View => "Views",
        }
    }
}

/// A database, as the tree's second level. `is_current` marks the one the
/// connection is actually attached to, which is the only one Postgres can read
/// without reconnecting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Catalog {
    pub name: String,
    pub is_current: bool,
}

/// Everything needed to name one table or view in a query, kept together so a
/// tree node can be turned into SQL without the caller reassembling it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectRef {
    pub catalog: Option<String>,
    pub schema: Option<String>,
    pub name: String,
    pub kind: ObjectKind,
}

impl ObjectRef {
    pub fn new(
        catalog: Option<String>,
        schema: Option<String>,
        name: impl Into<String>,
        kind: ObjectKind,
    ) -> Self {
        ObjectRef {
            catalog,
            schema,
            name: name.into(),
            kind,
        }
    }

    /// The name as SQL, quoted for the backend.
    ///
    /// The catalog is left out unless the backend can reach across databases on
    /// one connection: qualifying a Postgres table with its database name is a
    /// syntax error, not a no-op.
    pub fn qualified(&self, dialect: &Dialect) -> String {
        let mut parts: Vec<&str> = Vec::with_capacity(3);
        if dialect.backend.catalogs_share_connection()
            && let Some(catalog) = &self.catalog
        {
            parts.push(catalog);
        }
        if let Some(schema) = &self.schema {
            parts.push(schema);
        }
        parts.push(&self.name);
        dialect.quote_qualified(&parts)
    }

    /// The name as a human reads it in the status bar — unquoted, no catalog.
    pub fn display(&self) -> String {
        match &self.schema {
            Some(schema) => format!("{schema}.{}", self.name),
            None => self.name.clone(),
        }
    }
}
