//! The driver-agnostic half of binsql.
//!
//! Everything a database differs about is resolved inside an [`adapter`]; what
//! sits above this crate — today the TUI, later the command mode — works in
//! [`Session`]s, [`ResultSet`]s and [`ObjectRef`]s and never learns which
//! engine is on the other end.

pub mod adapter;
pub mod backend;
pub mod config;
pub mod dsn;
pub mod error;
pub mod schema;
pub mod schema_cache;
pub mod secrets;
pub mod session;
pub mod sql;
pub mod value;
pub mod workspace;

pub use adapter::Adapter;
pub use backend::{Backend, Dialect};
pub use config::{Config, DataSource};
pub use error::{Error, Result};
pub use schema::{Catalog, ObjectKind, ObjectRef};
pub use schema_cache::{Level, SchemaCache, SourceId};
pub use secrets::{Reference, Resolver};
pub use session::Session;
pub use sql::{Kind, Statement};
pub use value::{Column, ResultSet, Value};
pub use workspace::{Scope, Workspace};
