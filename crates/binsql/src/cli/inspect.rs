//! `binsql inspect` — what the tree shows, for something that cannot look.
//!
//! With no argument it lists the tables and views; with one it describes that
//! table's columns. Both answers are built as result sets, so every output
//! format the other verbs have works here for nothing. With `--columns` it
//! lists every object's columns at once, each row carrying the object it
//! belongs to, or one object's when it is named — exactly, since a name
//! under `--columns` is an identity rather than a guess.

use std::time::Instant;

use binsql_core::{Column, ObjectKind, ObjectRef, ResultSet, Session, Value};

use super::render;
use super::{
    Category, NOTHING_CHANGED, Result, caused, connect, core, cut_off, failed, note, output, parse,
    print, statement_budget, usage,
};

pub(super) const VALUES: &[&str] = &[];
pub(super) const SWITCHES: &[&str] = &["columns"];

pub async fn run(args: Vec<String>) -> Result<()> {
    let args = parse(args, VALUES, SWITCHES)?;
    let options = output(&args)?;
    let every_column = args.is_set(&["columns"]);
    let timeout = statement_budget(&args)?;

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

    if every_column {
        let (result, empty) = cut_off(
            timeout,
            NOTHING_CHANGED,
            columns_of(&session, &catalog, schema.as_deref(), target.as_deref()),
        )
        .await?;
        print(&render::rows(&result, &options))?;
        for object in empty {
            note(&options, &format!("{object} has no columns"));
        }
        return Ok(());
    }

    let result = cut_off(timeout, NOTHING_CHANGED, async {
        match target {
            Some(table) => describe(&session, &catalog, schema.as_deref(), &table).await,
            None => list(&session, &catalog, schema.as_deref()).await,
        }
    })
    .await?;

    print(&render::rows(&result, &options))
}

/// The schemas a call covers: the one named, or every one in the catalog. A
/// backend without a schema level answers with an empty list, and its objects
/// are reached by passing none, which `""` stands for here.
async fn scope(session: &Session, catalog: &str, schema: Option<&str>) -> Result<Vec<String>> {
    if let Some(schema) = schema {
        return Ok(vec![schema.to_string()]);
    }
    let found = session.schemas(catalog).await.map_err(core)?;
    Ok(if found.is_empty() {
        vec![String::new()]
    } else {
        found
    })
}

/// Every table and view in the schema, or in all of them when none was named.
async fn list(session: &Session, catalog: &str, schema: Option<&str>) -> Result<ResultSet> {
    let start = Instant::now();
    let schemas = scope(session, catalog, schema).await?;

    let mut rows = Vec::new();
    for schema in &schemas {
        let named = (!schema.is_empty()).then_some(schema.as_str());
        let objects = session.objects(catalog, named).await.map_err(core)?;

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
    let columns = session.columns(&object).await.map_err(core)?;

    let rows = columns.iter().map(describe_row).collect();

    Ok(ResultSet {
        columns: describe_columns(),
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
    let schemas = scope(session, catalog, schema).await?;

    for schema in &schemas {
        let named = (!schema.is_empty()).then_some(schema.as_str());
        let objects = session.objects(catalog, named).await.map_err(core)?;

        if let Some(object) = objects
            .into_iter()
            .find(|object| object.name.eq_ignore_ascii_case(name))
        {
            return Ok(object);
        }
    }

    Err(failed(format!("no table or view named {name} in {catalog}")).category(Category::Database))
}

/// Every object's columns in the schemas in scope, as one result, with the
/// objects that came back with no columns named so the caller can say so. The
/// whole of it is read before anything is printed, so a failure leaves stdout
/// empty.
async fn columns_of(
    session: &Session,
    catalog: &str,
    schema: Option<&str>,
    name: Option<&str>,
) -> Result<(ResultSet, Vec<String>)> {
    let start = Instant::now();
    let mut found = Vec::new();
    for schema in &scope(session, catalog, schema).await? {
        let named = (!schema.is_empty()).then_some(schema.as_str());
        found.extend(session.objects(catalog, named).await.map_err(core)?);
    }
    if let Some(name) = name {
        found = select(found, catalog, name)?;
    }

    let mut objects = Vec::new();
    for object in found {
        let columns = session
            .columns(&object)
            .await
            .map_err(|error| caused(format!("columns of {}: {error}", object.display()), &error))?;
        objects.push((object, columns));
    }

    let empty = objects
        .iter()
        .filter(|(_, columns)| columns.is_empty())
        .map(|(object, _)| object.display())
        .collect();

    let mut columns = vec![
        Column::new("catalog", "text"),
        Column::new("schema", "text"),
        Column::new("object", "text"),
        Column::new("kind", "text"),
    ];
    columns.extend(describe_columns());

    let result = ResultSet {
        columns,
        rows: column_rows(catalog, objects),
        rows_affected: None,
        elapsed: start.elapsed(),
        truncated: false,
    };
    Ok((result, empty))
}

/// The objects named exactly `name`, case and dots included. Finding none is
/// a refusal from the database's side, with any names that differ only in
/// case offered; finding it in more than one schema is the caller's to settle
/// with `--schema`, rather than ours to settle by picking one.
fn select(objects: Vec<ObjectRef>, catalog: &str, name: &str) -> Result<Vec<ObjectRef>> {
    let (matches, others): (Vec<_>, Vec<_>) =
        objects.into_iter().partition(|object| object.name == name);

    if matches.is_empty() {
        let mut near: Vec<_> = others
            .iter()
            .filter(|object| object.name.eq_ignore_ascii_case(name))
            .map(ObjectRef::display)
            .collect();
        near.sort();
        let mut message = format!("no table or view named {name} in {catalog}");
        if !near.is_empty() {
            message.push_str(&format!(
                "; names that differ only in case: {}",
                near.join(", ")
            ));
        }
        return Err(failed(message).category(Category::Database));
    }

    let mut schemas: Vec<_> = matches
        .iter()
        .map(|object| object.schema.clone().unwrap_or_default())
        .collect();
    schemas.sort();
    schemas.dedup();
    if schemas.len() > 1 {
        return Err(usage(format!(
            "{name} is in more than one schema ({}); pass --schema",
            schemas.join(", ")
        )));
    }
    Ok(matches)
}

/// One row per column, the object's identity in front of what `describe`
/// gives. Objects are sorted by schema (none first), name and kind, comparing
/// bytes, so every backend answers in the same order; columns keep the order
/// they were declared in. An object with no columns still gets a row, with
/// everything past its identity null, so it is not lost from the list.
fn column_rows(catalog: &str, mut objects: Vec<(ObjectRef, Vec<Column>)>) -> Vec<Vec<Value>> {
    objects.sort_by(|(a, _), (b, _)| {
        (&a.schema, &a.name, kind_label(a.kind)).cmp(&(&b.schema, &b.name, kind_label(b.kind)))
    });

    let mut rows = Vec::new();
    for (object, columns) in objects {
        let identity = vec![
            Value::Text(catalog.to_string()),
            object.schema.clone().map_or(Value::Null, Value::Text),
            Value::Text(object.name.clone()),
            Value::Text(kind_label(object.kind).to_string()),
        ];
        if columns.is_empty() {
            let mut row = identity.clone();
            row.extend(std::iter::repeat_n(Value::Null, 5));
            rows.push(row);
        }
        for column in &columns {
            let mut row = identity.clone();
            row.extend(describe_row(column));
            rows.push(row);
        }
    }
    rows
}

fn describe_columns() -> Vec<Column> {
    vec![
        Column::new("column", "text"),
        Column::new("type", "text"),
        Column::new("nullable", "bool"),
        Column::new("default", "text"),
        Column::new("primary_key", "bool"),
    ]
}

fn describe_row(column: &Column) -> Vec<Value> {
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
}

fn kind_label(kind: ObjectKind) -> &'static str {
    match kind {
        ObjectKind::Table => "table",
        ObjectKind::View => "view",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(schema: Option<&str>, name: &str, kind: ObjectKind) -> ObjectRef {
        ObjectRef::new(None, schema.map(str::to_string), name, kind)
    }

    fn text(value: &Value) -> Option<&str> {
        match value {
            Value::Text(text) => Some(text),
            _ => None,
        }
    }

    #[test]
    fn objects_are_sorted_and_columns_keep_their_order() {
        let rows = column_rows(
            "shop",
            vec![
                (
                    object(Some("sales"), "orders", ObjectKind::Table),
                    vec![Column::new("z", "int"), Column::new("a", "int")],
                ),
                (
                    object(Some("Sales"), "orders", ObjectKind::View),
                    vec![Column::new("id", "int")],
                ),
                (
                    object(None, "loose", ObjectKind::Table),
                    vec![Column::new("id", "int")],
                ),
                (
                    object(Some("sales"), "Orders", ObjectKind::View),
                    vec![Column::new("id", "int")],
                ),
            ],
        );

        let identity: Vec<_> = rows
            .iter()
            .map(|row| {
                (
                    text(&row[1]),
                    text(&row[2]).unwrap(),
                    text(&row[3]).unwrap(),
                    text(&row[4]).unwrap(),
                )
            })
            .collect();
        assert_eq!(
            identity,
            vec![
                (None, "loose", "table", "id"),
                (Some("Sales"), "orders", "view", "id"),
                (Some("sales"), "Orders", "view", "id"),
                (Some("sales"), "orders", "table", "z"),
                (Some("sales"), "orders", "table", "a"),
            ]
        );
        assert!(rows.iter().all(|row| text(&row[0]) == Some("shop")));
        assert_eq!(rows[0][1], Value::Null);
    }

    fn names(objects: &[ObjectRef]) -> Vec<String> {
        objects.iter().map(ObjectRef::display).collect()
    }

    #[test]
    fn a_name_is_matched_exactly() {
        let selected = select(
            vec![
                object(None, "Artist", ObjectKind::Table),
                object(None, "artist", ObjectKind::Table),
            ],
            "main",
            "artist",
        )
        .unwrap();
        assert_eq!(names(&selected), ["artist"]);
    }

    #[test]
    fn a_missing_name_offers_the_names_that_differ_only_in_case() {
        let failure = select(
            vec![
                object(Some("sales"), "Artist", ObjectKind::Table),
                object(Some("sales"), "ARTIST", ObjectKind::View),
            ],
            "shop",
            "artist",
        )
        .unwrap_err();
        assert!(!failure.usage);
        assert_eq!(
            failure.message,
            "no table or view named artist in shop; \
             names that differ only in case: sales.ARTIST, sales.Artist"
        );

        let failure = select(
            vec![object(None, "album", ObjectKind::Table)],
            "main",
            "artist",
        )
        .unwrap_err();
        assert!(!failure.usage);
        assert_eq!(failure.message, "no table or view named artist in main");
    }

    #[test]
    fn a_name_in_two_schemas_is_refused() {
        let failure = select(
            vec![
                object(Some("b"), "orders", ObjectKind::Table),
                object(Some("a"), "orders", ObjectKind::View),
                object(Some("b"), "orders", ObjectKind::View),
            ],
            "shop",
            "orders",
        )
        .unwrap_err();
        assert!(failure.usage);
        assert_eq!(
            failure.message,
            "orders is in more than one schema (a, b); pass --schema"
        );
    }

    #[test]
    fn a_name_with_a_dot_is_not_split() {
        let objects = vec![
            object(None, "a.b", ObjectKind::Table),
            object(Some("a"), "b", ObjectKind::Table),
        ];
        assert_eq!(names(&select(objects, "main", "a.b").unwrap()), ["a.b"]);
    }

    #[test]
    fn an_object_with_no_columns_is_one_row_of_nulls() {
        let rows = column_rows(
            "main",
            vec![(object(None, "gone", ObjectKind::View), Vec::new())],
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(text(&rows[0][2]), Some("gone"));
        assert_eq!(text(&rows[0][3]), Some("view"));
        assert!(rows[0][4..].iter().all(|value| *value == Value::Null));
    }
}
