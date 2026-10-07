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
