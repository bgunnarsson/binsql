//! The credential store, for real.
//!
//! Ignored by default: this writes to the developer's own keychain, which a
//! plain `cargo test` has no business doing. Run it deliberately with
//! `cargo test -p binsql-core --test keychain_roundtrip -- --ignored`.

use binsql_core::Resolver;
use binsql_core::secrets::{Cache, keychain};

/// A name no real connection would take, so a failed run leaves nothing
/// mistakable for someone's data source behind.
fn account() -> String {
    format!("binsql-test/{}", std::process::id())
}

#[tokio::test]
#[ignore = "writes to the operating system credential store"]
async fn a_stored_connection_string_resolves_and_can_be_moved_and_removed() {
    let account = account();
    let dsn = "sqlserver://sa:hunter2@localhost:1433/app";

    keychain::set(&account, dsn).expect("filing the secret");

    // What the config would hold resolves back to what was filed.
    let resolver = Resolver::new(Cache::new(std::env::temp_dir(), Default::default()), None);
    let resolved = resolver
        .resolve(&keychain::reference(&account))
        .await
        .expect("resolving the reference");
    assert_eq!(resolved, dsn);

    // A rename takes the secret with it.
    let renamed = format!("{account}-renamed");
    keychain::rename(&account, &renamed).expect("moving the secret");
    assert_eq!(
        keychain::get(&renamed).expect("reading the moved secret"),
        dsn
    );
    assert!(
        keychain::get(&account).is_err(),
        "the old account should be gone"
    );

    keychain::delete(&renamed).expect("removing the secret");
    assert!(keychain::get(&renamed).is_err(), "it should be gone");
    // Removing what is not there is not a failure.
    keychain::delete(&renamed).expect("removing it twice");
}

#[tokio::test]
#[ignore = "writes to the operating system credential store"]
async fn a_missing_entry_says_how_to_put_it_back() {
    let error = keychain::get(&format!("{}-never-filed", account()))
        .expect_err("nothing was filed under that name");
    let message = error.to_string();
    assert!(message.contains(keychain::STORE_NAME), "{message}");
    assert!(message.contains("connection string again"), "{message}");
}
