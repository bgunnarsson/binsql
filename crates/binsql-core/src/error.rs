use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The connection string does not name a backend binsql can drive.
    #[error("unrecognised connection string: {0}")]
    UnknownBackend(String),

    #[error("connecting to {name}: {source}")]
    Connect {
        name: String,
        #[source]
        source: anyhow::Error,
        /// Set when binsql knows which part of connecting failed.
        reason: Option<Reason>,
    },

    #[error("{0}")]
    Query(#[source] anyhow::Error),

    /// A write attempted against a data source registered read-only. This is a
    /// refusal, not a failure: nothing was sent to the server.
    #[error("{data_source} is registered read-only; refusing to run {statement}")]
    ReadOnly {
        data_source: String,
        statement: String,
    },

    /// A plan asked for something other than one statement that reads. Like
    /// [`Error::ReadOnly`] this is a refusal: nothing was sent to the server.
    #[error(
        "only one SELECT, WITH, VALUES or TABLE statement can be planned; refusing to plan {statement}"
    )]
    NotPlannable { statement: String },

    /// The values given do not fill the placeholders one for one. Found before
    /// anything is sent, like [`Error::ReadOnly`].
    #[error("{} in the SQL, but {} to bind", plural(.found, "placeholder"), plural(.given, "value"))]
    Placeholders { found: usize, given: usize },

    /// The caller asked for the query to stop. Like [`Error::ReadOnly`] this is
    /// an outcome rather than a fault, and nothing about the database is wrong.
    #[error("cancelled")]
    Cancelled,

    #[error("{0}")]
    Config(#[source] anyhow::Error),

    /// A secret could not be read. The text is the one the parts always made;
    /// they are kept apart for a caller that wants the next step on its own.
    #[error("{}", secret_text(.reference, .reason, .hint, .detail))]
    Secret {
        reference: String,
        reason: Option<Reason>,
        hint: Option<String>,
        /// What the tool that was asked said, as it said it.
        detail: String,
    },

    #[error("{0}")]
    Io(#[from] std::io::Error),

    /// A batch failed after its transaction began. The text is the failure's
    /// own; `outcome` says what became of the transaction. An error from a
    /// batch that is not this one came before anything in it ran.
    #[error("{error}")]
    Transaction {
        error: Box<Error>,
        outcome: TransactionOutcome,
        /// The 1-based position of the statement that failed, or `None` when
        /// the batch ran and its commit or rollback did not.
        statement: Option<usize>,
    },
}

impl Error {
    pub fn query(err: impl Into<anyhow::Error>) -> Self {
        Error::Query(err.into())
    }

    pub fn connect(name: impl fmt::Display, err: impl Into<anyhow::Error>) -> Self {
        Error::Connect {
            name: name.to_string(),
            source: err.into(),
            reason: None,
        }
    }

    /// The Azure AD token for a `fedauth=` connection could not be had.
    pub fn token(err: impl Into<anyhow::Error>) -> Self {
        Error::Connect {
            name: "azure ad".to_string(),
            source: err.into(),
            reason: Some(Reason::AzureAdToken),
        }
    }

    pub fn config(err: impl Into<anyhow::Error>) -> Self {
        Error::Config(err.into())
    }

    pub fn transaction(
        error: Error,
        outcome: TransactionOutcome,
        statement: Option<usize>,
    ) -> Self {
        Error::Transaction {
            error: Box::new(error),
            outcome,
            statement,
        }
    }

    /// The database's own code for a failure, when the driver gives it as a
    /// typed field: the SQLSTATE on PostgreSQL and MySQL, the extended result
    /// code on SQLite and the error number on SQL Server. Never read from the
    /// message.
    pub fn native_code(&self) -> Option<String> {
        let source = match self {
            Error::Query(source) | Error::Connect { source, .. } => source,
            Error::Transaction { error, .. } => return error.native_code(),
            _ => return None,
        };
        source.chain().find_map(|cause| {
            if let Some(error) = cause.downcast_ref::<sqlx::Error>() {
                return error
                    .as_database_error()
                    .and_then(|error| error.code())
                    .map(|code| code.into_owned());
            }
            cause
                .downcast_ref::<tiberius::error::Error>()
                .and_then(|error| error.code())
                .map(|code| code.to_string())
        })
    }
}

/// Why a secret or a token could not be had, finer than the variant. A
/// caller reading these must treat one it does not know as no reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    AzMissing,
    AzUnauthenticated,
    VaultForbidden,
    SecretNotFound,
    VaultNotFound,
    AzureAdToken,
}

impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Reason::AzMissing => "az-missing",
            Reason::AzUnauthenticated => "az-unauthenticated",
            Reason::VaultForbidden => "vault-forbidden",
            Reason::SecretNotFound => "secret-not-found",
            Reason::VaultNotFound => "vault-not-found",
            Reason::AzureAdToken => "azure-ad-token",
        }
    }
}

/// What became of a transaction whose batch failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionOutcome {
    /// The rollback was confirmed: nothing was kept.
    RolledBack,
    /// The commit or the rollback failed, or the connection went with the
    /// transaction, so what was kept cannot be said.
    Unknown,
}

impl TransactionOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            TransactionOutcome::RolledBack => "rolled_back",
            TransactionOutcome::Unknown => "unknown",
        }
    }
}

fn secret_text(
    reference: &str,
    reason: &Option<Reason>,
    hint: &Option<String>,
    detail: &str,
) -> String {
    match (reason, hint) {
        (Some(Reason::AzMissing), _) => {
            format!("running `az`: {detail}. Is the Azure CLI installed?")
        }
        (_, Some(hint)) => format!("reading {reference}: {detail}\n\n{hint}"),
        (_, None) => format!("reading {reference}: {detail}"),
    }
}

fn plural(count: &usize, noun: &str) -> String {
    if *count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_database_error_carries_the_drivers_code() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query("CREATE TABLE t (id INTEGER PRIMARY KEY)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO t VALUES (1)")
            .execute(&pool)
            .await
            .unwrap();
        let failure = sqlx::query("INSERT INTO t VALUES (1)")
            .execute(&pool)
            .await
            .unwrap_err();
        let expected = failure
            .as_database_error()
            .unwrap()
            .code()
            .unwrap()
            .into_owned();

        assert_eq!(Error::query(failure).native_code(), Some(expected));
    }

    #[test]
    fn an_error_without_a_driver_code_has_none() {
        assert_eq!(Error::query(anyhow::anyhow!("no rows")).native_code(), None);
        assert_eq!(
            Error::connect("db", anyhow::anyhow!("refused")).native_code(),
            None
        );
        assert_eq!(Error::Cancelled.native_code(), None);
    }
}
