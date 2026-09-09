//! `binsql query` — run one read-only statement and print what it returned.
//!
//! The verb is the safety boundary. `query` refuses to write, so a script that
//! only ever reads can say so in the command it runs, and anyone reading that
//! script later can see it without reading the SQL.

use binsql_core::sql;

use super::render;
use super::{Result, cancel_on_interrupt, connect, failed, output, parse, print, read_sql, usage};

const VALUES: &[&str] = &["file", "f", "limit"];
const SWITCHES: &[&str] = &["allow-write"];

pub async fn run(args: Vec<String>) -> Result<()> {
    let args = parse(args, VALUES, SWITCHES)?;
    let options = output(&args)?;
    let limit = match args.value(&["limit"]) {
        Some(value) => match value.parse::<usize>() {
            Ok(0) => None,
            Ok(limit) => Some(limit),
            Err(_) => return Err(usage(format!("--limit wants a number, got {value}"))),
        },
        None => None,
    };

    let script = read_sql(&args)?;
    let session = connect(&args).await?;

    // Vetted after connecting, because the dialect decides how the script
    // splits — but before anything is sent, so a refusal costs the server
    // nothing.
    let backend = session.backend();
    let statements = sql::split(&script, backend);
    let [statement] = statements.as_slice() else {
        return Err(match statements.len() {
            0 => usage("no SQL statement found"),
            count => usage(format!(
                "query runs one statement; this is {count} — use `binsql exec` for a script"
            )),
        });
    };

    if statement.kind.mutates() && !args.is_set(&["allow-write"]) {
        return Err(usage(format!(
            "refusing to run a {} statement with `query`: use `binsql exec`, or --allow-write\n  statement: {}",
            statement.kind.label(),
            sql::summarize(&statement.sql, backend, 80)
        )));
    }

    let cancel = cancel_on_interrupt();
    let result = session
        .run_cancellable(None, &statement.sql, limit, &cancel)
        .await
        .map_err(|error| failed(error.to_string()))?;

    print(&render::rows(&result, &options))
}
