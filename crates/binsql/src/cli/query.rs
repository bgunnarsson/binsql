//! `binsql query` — run one read-only statement and print what it returned.
//!
//! The verb is the safety boundary. `query` refuses to write, so a script that
//! only ever reads can say so in the command it runs, and anyone reading that
//! script later can see it without reading the SQL.

use binsql_core::sql;

use super::render;
use super::{
    Category, GRACE, Phase, Result, Stop, Transaction, bind_values, connect, failed, output, parse,
    print, read_sql, statement_budget, usage,
};

pub(super) const VALUES: &[&str] = &["file", "f", "limit", "arg", "timeout-ms"];
pub(super) const SWITCHES: &[&str] = &["allow-write", "plan", "require-rows"];

/// What a timed-out read leaves behind.
const NOTHING_CHANGED: &str = "nothing was changed by binsql";

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

    let require_rows = args.is_set(&["require-rows"]);
    if require_rows && args.is_set(&["plan"]) {
        return Err(usage(
            "--require-rows checks what a query returns, and a plan always has rows; drop one",
        ));
    }

    let timeout = statement_budget(&args)?;
    let params = bind_values(&args)?;
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
        }
        .phase(Phase::Prepare));
    };

    // The clock starts once connected: --connect-timeout-ms is the budget
    // for getting this far.
    let stop = Stop::start(timeout);
    if args.is_set(&["plan"]) {
        // Checked here as well as in the core, so a refusal is a usage error
        // and can name the switch.
        if !params.is_empty() {
            return Err(usage("--plan plans the statement as written; drop --arg"));
        }
        if !sql::plannable(&statement.sql, backend) {
            let summary = sql::summarize(&statement.sql, backend, 80);
            return Err(usage(if statement.kind.mutates() {
                format!(
                    "--plan plans reads only; drop --plan to run a {} statement",
                    statement.kind.label()
                )
            } else {
                "--plan plans one SELECT, WITH, VALUES or TABLE statement".to_string()
            })
            .phase(Phase::Prepare)
            .context(format!("statement: {summary}")));
        }
        let result = stop
            .run(
                GRACE,
                NOTHING_CHANGED,
                session.plan(None, &statement.sql, limit, stop.token()),
            )
            .await?;
        return print(&render::rows(&result, &options));
    }

    if statement.kind.mutates() && !args.is_set(&["allow-write"]) {
        return Err(usage(format!(
            "refusing to run a {} statement with `query`: use `binsql exec`, or --allow-write",
            statement.kind.label(),
        ))
        .category(Category::Refused)
        .phase(Phase::Prepare)
        .statement(1)
        .context(format!(
            "statement: {}",
            sql::summarize(&statement.sql, backend, 80)
        )));
    }

    let bound = sql::bind(std::slice::from_ref(statement), &params, backend)
        .map_err(|error| usage(format!("{error} — one --arg per ?")).phase(Phase::Prepare))?;

    // A write let through by --allow-write may have landed before the cancel
    // did, so its timeout cannot promise a rollback, and says so in JSON too.
    let writes = statement.kind.mutates();
    let known = if writes {
        "whether the statement took effect is unknown"
    } else {
        NOTHING_CHANGED
    };
    let result = stop
        .run(
            GRACE,
            known,
            session.run_bound(None, &bound[0], limit, stop.token()),
        )
        .await
        .map_err(|failure| match failure.category {
            Category::Timeout if writes => failure.transaction(Transaction::Unknown),
            _ => failure,
        })?;

    print(&render::rows(&result, &options))?;
    // Checked after printing, so the result reads the same whichever way the
    // assertion goes.
    if require_rows && result.rows.is_empty() {
        return Err(failed("the query returned no rows (--require-rows)")
            .category(Category::Assertion)
            .phase(Phase::Output));
    }
    Ok(())
}
