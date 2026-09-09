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

    #[error("{0}")]
    Config(#[source] anyhow::Error),

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
        }
    }

    pub fn config(err: impl Into<anyhow::Error>) -> Self {
        Error::Config(err.into())
    }
}
