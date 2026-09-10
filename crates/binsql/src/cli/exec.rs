//! `binsql exec` — run statements that change the database.
//!
//! Three things stand between a script and a database it should not have done
//! that to: a batch is one transaction unless told otherwise, `--dry-run` runs
//! it and keeps nothing, and the statements most likely to be a mistake need
//! `--force` before they are sent.

use binsql_core::{Backend, Statement, sql};

use super::render::{self, Outcome};
use super::{
    Result, bind_values, cancel_on_interrupt, connect, failed, note, output, parse, print,
    read_sql, usage,
};

const VALUES: &[&str] = &["file", "f", "arg"];
const SWITCHES: &[&str] = &["dry-run", "tx", "no-tx", "force"];

pub async fn run(args: Vec<String>) -> Result<()> {
    let args = parse(args, VALUES, SWITCHES)?;
    let options = output(&args)?;

    let dry_run = args.is_set(&["dry-run"]);
    let force_tx = args.is_set(&["tx"]);
    let no_tx = args.is_set(&["no-tx"]);
    if force_tx && no_tx {
        return Err(usage("--tx and --no-tx contradict each other"));
    }
    if dry_run && no_tx {
        return Err(usage(
            "--dry-run is a transaction that is rolled back, so it cannot be combined with --no-tx",
        ));
    }

    let params = bind_values(&args)?;
    let script = read_sql(&args)?;
    let session = connect(&args).await?;
    let backend = session.backend();

    let statements = sql::split(&script, backend);
    if statements.is_empty() {
        return Err(usage("no SQL statement found"));
    }
    if !args.is_set(&["force"]) {
        refuse_the_obviously_destructive(&statements, backend)?;
    }
    let bound = sql::bind(&statements, &params, backend)
        .map_err(|error| usage(format!("{error} — one --arg per ?")))?;

    // A batch is one unit of work by default; a lone statement is not worth
    // wrapping. Either way `--tx` and `--dry-run` say so outright.
    let transactional = (statements.len() > 1 || force_tx || dry_run) && !no_tx;
    if dry_run {
        warn_about_implicit_commits(&statements, backend, &options);
    }

    let cancel = cancel_on_interrupt();
    let results = if transactional {
        session
            .run_transaction(None, &bound, None, !dry_run, &cancel)
            .await
            .map_err(|error| {
                failed(format!(
                    "{error}\n  the transaction was rolled back; nothing was kept"
                ))
            })?
    } else {
        let mut results = Vec::with_capacity(bound.len());
        for (index, (statement, bound)) in statements.iter().zip(&bound).enumerate() {
            let result = session
                .run_bound(None, bound, None, &cancel)
                .await
                .map_err(|error| {
                    // Without a transaction there is nothing to undo, so the
                    // message has to say how far the batch got.
                    let stranded = match index {
                        0 => String::new(),
                        1 => "\n  1 earlier statement already ran and was not rolled back".into(),
                        count => format!(
                            "\n  {count} earlier statements already ran and were not rolled back"
                        ),
                    };
                    failed(format!(
                        "{error}\n  statement: {}{stranded}",
                        sql::summarize(&statement.sql, backend, 100)
                    ))
                })?;
            results.push(result);
        }
        results
    };

    let outcomes: Vec<Outcome> = statements
        .iter()
        .zip(results)
        .map(|(statement, result)| Outcome {
            sql: sql::summarize(&statement.sql, backend, 100),
            kind: statement.kind.label(),
            result,
        })
        .collect();

    print(&render::outcomes(&outcomes, !dry_run, &options))
}

/// Refuses the two mistakes that cannot be undone by noticing them afterwards.
fn refuse_the_obviously_destructive(statements: &[Statement], backend: Backend) -> Result<()> {
    for (index, statement) in statements.iter().enumerate() {
        let first = sql::summarize(&statement.sql, backend, 0)
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_uppercase();

        let reason = match first.as_str() {
            "UPDATE" | "DELETE" if !sql::has_keyword(&statement.sql, backend, "WHERE") => {
                Some(format!("{first} with no WHERE clause changes every row"))
            }
            "DROP" | "TRUNCATE" => Some(format!("{first} discards data for good")),
            _ => None,
        };

        if let Some(reason) = reason {
            return Err(usage(format!(
                "refusing statement {}: {reason} — pass --force if that is what you meant\n  statement: {}",
                index + 1,
                sql::summarize(&statement.sql, backend, 100)
            )));
        }
    }
    Ok(())
}

/// MySQL commits DDL implicitly, so a dry run there cannot put a schema change
/// back. Better said out loud than discovered.
fn warn_about_implicit_commits(
    statements: &[Statement],
    backend: Backend,
    options: &render::Options,
) {
    if backend != Backend::MySql {
        return;
    }
    if statements
        .iter()
        .any(|statement| statement.kind == binsql_core::Kind::Ddl)
    {
        note(
            options,
            "warning: MySQL commits DDL as it runs it — --dry-run cannot roll back the schema \
             changes in this batch",
        );
    }
}
