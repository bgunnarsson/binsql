//! `binsql source` — the saved data sources, read and changed from the command
//! line.
//!
//! It takes its own flags rather than the shared ones: `--conn`, `--catalog`
//! and `--schema` name a database to talk to, and nothing here talks to one.

use super::args::Args;
use super::{Result, usage};

const VALUES: &[&str] = &["format", "o"];
const SWITCHES: &[&str] = &["pretty", "no-header", "no-footer"];

pub async fn run(args: Vec<String>) -> Result<()> {
    let args = Args::parse(args, VALUES, SWITCHES)?;
    match args.positional().first().map(String::as_str) {
        None => Err(usage("source needs a command: list or show")),
        Some(other) => Err(usage(format!("unknown source command {other}"))),
    }
}
