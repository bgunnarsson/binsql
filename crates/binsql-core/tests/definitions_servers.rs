//! Reading definitions from the servers, which a plain `cargo test` has none
//! of.
//!
//! Each test reads a connection string from the environment and is ignored by
//! default. Point them at throwaway databases — each creates and drops objects
//! named `binsql_definition*` — and run them deliberately:
//!
//! ```sh
//! BINSQL_TEST_POSTGRES=postgres://… \
//!     cargo test -p binsql-core --test definitions_servers -- --ignored
//! ```

use binsql_core::{
    Backend, DataSource, Definition, DefinitionForm, ObjectKind, ObjectRef, Session, sql,
};
use tokio_util::sync::CancellationToken;

async fn open(variable: &str, backend: Backend) -> Session {
    let dsn = std::env::var(variable)
        .unwrap_or_else(|_| panic!("{variable} names the database to test against"));
    let source = DataSource {
        backend,
        dsn,
        description: String::new(),
        read_only: false,
        open_on_start: false,
    };
    Session::open(variable, source).await.expect("connect")
}

async fn run(session: &Session, sql: &str) {
    let backend = session.backend();
    let bound = sql::bind(&sql::split(sql, backend), &[], backend).expect("placeholders");
    session
        .run_bound(None, &bound[0], None, &CancellationToken::new())
        .await
        .unwrap_or_else(|error| panic!("{sql}: {error}"));
}

async fn definition(session: &Session, schema: &str, name: &str, kind: ObjectKind) -> Definition {
    let object = ObjectRef::new(None, Some(schema.into()), name, kind);
    session
        .definition(&object)
        .await
        .unwrap_or_else(|error| panic!("{name}: {error}"))
}

#[tokio::test]
#[ignore = "needs a Postgres server in BINSQL_TEST_POSTGRES"]
async fn postgres_views_give_their_query_and_tables_none() {
    let session = open("BINSQL_TEST_POSTGRES", Backend::Postgres).await;
    run(
        &session,
        "DROP MATERIALIZED VIEW IF EXISTS public.binsql_definition_m",
    )
    .await;
    run(&session, "DROP VIEW IF EXISTS public.binsql_definition_v").await;
    run(&session, "DROP TABLE IF EXISTS public.binsql_definition").await;
    run(
        &session,
        "CREATE TABLE public.binsql_definition (id int PRIMARY KEY)",
    )
    .await;
    run(
        &session,
        "CREATE VIEW public.binsql_definition_v AS SELECT id FROM public.binsql_definition",
    )
    .await;
    run(
        &session,
        "CREATE MATERIALIZED VIEW public.binsql_definition_m AS SELECT id FROM public.binsql_definition",
    )
    .await;

    for name in ["binsql_definition_v", "binsql_definition_m"] {
        let view = definition(&session, "public", name, ObjectKind::View).await;
        assert_eq!(view.form, DefinitionForm::Query, "{name}");
        let text = view.text.expect("a view's query");
        assert!(text.contains("binsql_definition"), "{name}: {text}");
        assert!(!text.to_uppercase().contains("CREATE"), "{name}: {text}");
    }
    let table = definition(&session, "public", "binsql_definition", ObjectKind::Table).await;
    assert_eq!(
        table,
        Definition {
            form: DefinitionForm::Unsupported,
            text: None,
        }
    );
    let missing = ObjectRef::new(
        None,
        Some("public".into()),
        "binsql_definition_absent",
        ObjectKind::Table,
    );
    assert!(session.definition(&missing).await.is_err());

    run(
        &session,
        "DROP MATERIALIZED VIEW public.binsql_definition_m",
    )
    .await;
    run(&session, "DROP VIEW public.binsql_definition_v").await;
    run(&session, "DROP TABLE public.binsql_definition").await;
}
