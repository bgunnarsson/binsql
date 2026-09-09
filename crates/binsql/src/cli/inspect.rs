//! `binsql inspect` — what the tree shows, for something that cannot look.
//!
//! With no argument it lists the tables and views; with one it describes that
//! table's columns. Both answers are built as result sets, so every output
//! format the other verbs have works here for nothing.

use std::time::Instant;

use binsql_core::{Column, ObjectKind, ObjectRef, ResultSet, Session, Value};

use super::render;
use super::{Result, connect, failed, output, parse, print, usage};

pub async fn run(args: Vec<String>) -> Result<()> {
    let args = parse(args, &[], &[])?;
    let options = output(&args)?;

    let target = match args.positional() {
        [] => None,
        [name] => Some(name.clone()),
        _ => return Err(usage("inspect describes one table at a time")),
    };

    let session = connect(&args).await?;
    let catalog = match args.value(&["catalog"]) {
        Some(catalog) => catalog.to_string(),
        None => session
            .current_catalog()
            .ok_or_else(|| usage("this data source has no current database — pass --catalog"))?
            .to_string(),
    };
    let schema = args.value(&["schema"]).map(str::to_string);

    let result = match target {
        Some(table) => describe(&session, &catalog, schema.as_deref(), &table).await?,
        None => list(&session, &catalog, schema.as_deref()).await?,
    };

    print(&render::rows(&result, &options))
}

/// Every table and view in the schema, or in all of them when none was named.
async fn list(session: &Session, catalog: &str, schema: Option<&str>) -> Result<ResultSet> {
    let start = Instant::now();
    let schemas = match schema {
        Some(schema) => vec![schema.to_string()],
        None => {
            let found = session
                .schemas(catalog)
                .await
                .map_err(|error| failed(error.to_string()))?;
            // A backend without a schema level answers with an empty list, and
            // its objects are reached by passing none.
            if found.is_empty() {
                vec![String::new()]
            } else {
                found
            }
        }
    };

    let mut rows = Vec::new();
    for schema in &schemas {
        let named = (!schema.is_empty()).then_some(schema.as_str());
        let objects = session
            .objects(catalog, named)
            .await
            .map_err(|error| failed(error.to_string()))?;

        for object in objects {
            rows.push(vec![
                Value::Text(object.schema.clone().unwrap_or_default()),
                Value::Text(object.name.clone()),
                Value::Text(kind_label(object.kind).to_string()),
            ]);
        }
    }

    Ok(ResultSet {
        columns: vec![
            Column::new("schema", "text"),
            Column::new("name", "text"),
            Column::new("kind", "text"),
        ],
        rows,
        rows_affected: None,
        elapsed: start.elapsed(),
        truncated: false,
    })
}

/// One table's columns. The name may carry its own schema — `dbo.orders` —
/// which wins over `--schema`, since it is the more specific of the two.
async fn describe(
    session: &Session,
    catalog: &str,
    schema: Option<&str>,
    table: &str,
) -> Result<ResultSet> {
    let start = Instant::now();
    let (schema, name) = match table.split_once('.') {
        Some((schema, name)) => (Some(schema.to_string()), name.to_string()),
        None => (schema.map(str::to_string), table.to_string()),
    };

    let object = find(session, catalog, schema.as_deref(), &name).await?;
    let columns = session
        .columns(&object)
        .await
        .map_err(|error| failed(error.to_string()))?;

    let rows = columns
        .iter()
        .map(|column| {
            vec![
                Value::Text(column.name.clone()),
                Value::Text(column.type_name.clone()),
                match column.nullable {
                    Some(nullable) => Value::Bool(nullable),
                    None => Value::Null,
                },
                match &column.default {
                    Some(default) => Value::Text(default.clone()),
                    None => Value::Null,
                },
                Value::Bool(column.primary_key),
            ]
        })
        .collect();

    Ok(ResultSet {
        columns: vec![
            Column::new("column", "text"),
            Column::new("type", "text"),
            Column::new("nullable", "bool"),
            Column::new("default", "text"),
            Column::new("primary_key", "bool"),
        ],
        rows,
        rows_affected: None,
        elapsed: start.elapsed(),
        truncated: false,
    })
}

/// Finds the table by asking the server what it has, rather than trusting the
/// name to be spelled the way the catalogue spells it — which is also how a
/// missing table gets a better error than the query would have given it.
async fn find(
    session: &Session,
    catalog: &str,
    schema: Option<&str>,
    name: &str,
) -> Result<ObjectRef> {
    let schemas = match schema {
        Some(schema) => vec![schema.to_string()],
        None => {
            let found = session
                .schemas(catalog)
                .await
                .map_err(|error| failed(error.to_string()))?;
            if found.is_empty() {
                vec![String::new()]
            } else {
                found
            }
        }
    };

    for schema in &schemas {
        let named = (!schema.is_empty()).then_some(schema.as_str());
        let objects = session
            .objects(catalog, named)
            .await
            .map_err(|error| failed(error.to_string()))?;

        if let Some(object) = objects
            .into_iter()
            .find(|object| object.name.eq_ignore_ascii_case(name))
        {
            return Ok(object);
        }
    }

    Err(failed(format!(
        "no table or view named {name} in {catalog}"
    )))
}

fn kind_label(kind: ObjectKind) -> &'static str {
    match kind {
        ObjectKind::Table => "table",
        ObjectKind::View => "view",
    }
}
