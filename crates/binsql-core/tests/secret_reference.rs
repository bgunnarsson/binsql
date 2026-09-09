//! A data source can name a secret instead of holding one. This exercises the
//! whole path — reference, cache, connection — without touching a vault, by
//! seeding the cache with the connection string a vault would have returned.

use std::time::Duration;

use binsql_core::secrets::{Cache, DEFAULT_TTL, Reference, Resolver};
use binsql_core::{Backend, DataSource, Session};

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("binsql-secret-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn source(dsn: &str) -> DataSource {
    DataSource {
        backend: Backend::Sqlite,
        dsn: dsn.to_string(),
        description: String::new(),
        read_only: false,
        open_on_start: false,
    }
}

#[tokio::test]
async fn a_reference_resolves_and_the_session_opens() {
    let dir = scratch("opens");
    let database = dir.join("app.db");
    // SQLite reads a zero-length file as an empty database.
    std::fs::write(&database, b"").expect("create database");

    let resolver = Resolver::new(Cache::new(&dir, DEFAULT_TTL), None);
    let reference = Reference::parse("keyvault://kv-demo/app-dsn", None).expect("parse reference");

    // Stand in for the vault: the value a fetch would have returned.
    resolver
        .cache()
        .put(&reference, &database.display().to_string())
        .expect("seed the cache");

    let session = Session::open_with("prod", source("keyvault://kv-demo/app-dsn"), &resolver)
        .await
        .expect("session opens from a reference");

    session
        .run(None, "CREATE TABLE t (a INTEGER)", None)
        .await
        .expect("the connection is usable");

    let catalogs = session.catalogs().await.expect("introspection works");
    assert!(catalogs.iter().any(|catalog| catalog.name == "main"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_literal_dsn_is_left_alone() {
    let dir = scratch("literal");
    let database = dir.join("app.db");
    std::fs::write(&database, b"").expect("create database");

    let resolver = Resolver::new(Cache::new(&dir, DEFAULT_TTL), None);
    Session::open_with("local", source(&database.display().to_string()), &resolver)
        .await
        .expect("a plain path still opens");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_zero_ttl_resolves_without_writing_to_disk() {
    let dir = scratch("nocache");
    let resolver = Resolver::new(Cache::new(&dir, Duration::ZERO), None);
    let reference = Reference::parse("keyvault://kv-demo/app-dsn", None).expect("parse reference");

    resolver.cache().put(&reference, "ignored").expect("no-op");
    assert!(
        !dir.join("secret-cache.json").exists(),
        "a zero TTL must keep secrets off the disk"
    );
    assert_eq!(resolver.cache().count(), 0);

    let _ = std::fs::remove_dir_all(&dir);
}
