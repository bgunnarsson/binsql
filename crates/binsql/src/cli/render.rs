//! Turning a result set into the shape the caller asked for.
//!
//! Command mode's whole job is to hand a result to something else — a shell, a
//! script, an agent — so the format is the interface, and every one of them is
//! written out here rather than borrowed from the grid the TUI draws.

use std::fmt::Write as _;

use binsql_core::{Column, ResultSet, Value};
use serde_json::{Map, Number, Value as Json, json};
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Table,
    Json,
    Jsonl,
    Csv,
    Tsv,
    Vertical,
    Markdown,
    Raw,
    None,
}

impl Format {
    pub const NAMES: &'static str = "table, json, jsonl, csv, tsv, vertical, markdown, raw, none";

    pub fn parse(name: &str) -> Option<Format> {
        match name.trim().to_ascii_lowercase().as_str() {
            "" | "table" => Some(Format::Table),
            "json" => Some(Format::Json),
            "jsonl" | "ndjson" => Some(Format::Jsonl),
            "csv" => Some(Format::Csv),
            "tsv" => Some(Format::Tsv),
            "vertical" | "expanded" => Some(Format::Vertical),
            "markdown" | "md" => Some(Format::Markdown),
            "raw" | "plain" => Some(Format::Raw),
            "none" | "quiet" => Some(Format::None),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Options {
    pub format: Format,
    pub pretty: bool,
    /// Whether the formats that have a header row print one.
    pub header: bool,
    /// Whether the formats meant for a human print the trailing row count.
    pub footer: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            format: Format::Table,
            pretty: false,
            header: true,
            footer: true,
        }
    }
}

/// The widest a table column is allowed to get before its values are cut.
const MAX_COLUMN_WIDTH: usize = 60;
const NULL: &str = "NULL";

pub fn rows(result: &ResultSet, options: &Options) -> String {
    match options.format {
        Format::Table => table(result, options),
        Format::Json => json_object(result, options),
        Format::Jsonl => jsonl(result),
        Format::Csv => separated(result, ',', options),
        Format::Tsv => separated(result, '\t', options),
        Format::Vertical => vertical(result, options),
        Format::Markdown => markdown(result),
        Format::Raw => raw(result),
        Format::None => String::new(),
    }
}

/// What one statement did, for `exec`'s report.
pub struct Outcome {
    pub sql: String,
    pub kind: &'static str,
    pub result: ResultSet,
}

/// Reports a batch: what each statement did, and whether any of it was kept.
pub fn outcomes(outcomes: &[Outcome], committed: bool, options: &Options) -> String {
    if options.format == Format::None {
        return String::new();
    }

    if matches!(options.format, Format::Json | Format::Jsonl) {
        let statements: Vec<Json> = outcomes
            .iter()
            .map(|outcome| {
                let mut entry = Map::new();
                entry.insert("sql".into(), json!(outcome.sql));
                entry.insert("kind".into(), json!(outcome.kind));
                entry.insert(
                    "rows_affected".into(),
                    match outcome.result.rows_affected {
                        Some(count) => json!(count),
                        None => Json::Null,
                    },
                );
                entry.insert("duration_ms".into(), json!(milliseconds(&outcome.result)));
                if !outcome.result.columns.is_empty() {
                    entry.insert("rows".into(), Json::Array(row_objects(&outcome.result)));
                }
                Json::Object(entry)
            })
            .collect();

        let report = json!({
            "statements": statements,
            "committed": committed,
            "rows_affected": outcomes
                .iter()
                .filter_map(|outcome| outcome.result.rows_affected)
                .sum::<u64>(),
        });
        return encode(&report, options.pretty);
    }

    let mut out = String::new();
    for outcome in outcomes {
        // A statement that returned rows is worth showing; one that did not is
        // worth counting.
        if !outcome.result.columns.is_empty() {
            let _ = writeln!(out, "{}", outcome.sql);
            out.push_str(&rows(&outcome.result, options));
            continue;
        }
        match outcome.result.rows_affected {
            Some(count) => {
                let noun = if count == 1 { "row" } else { "rows" };
                let _ = writeln!(out, "{}  — {count} {noun}", outcome.sql);
            }
            None => {
                let _ = writeln!(out, "{}  — ok", outcome.sql);
            }
        }
    }

    if options.footer {
        let total: u64 = outcomes
            .iter()
            .filter_map(|outcome| outcome.result.rows_affected)
            .sum();
        let noun = if total == 1 { "row" } else { "rows" };
        let ending = if committed {
            "committed"
        } else {
            "rolled back — nothing was kept"
        };
        let _ = writeln!(out, "({total} {noun} affected, {ending})");
    }

    out
}

// --- formats -------------------------------------------------------------

fn table(result: &ResultSet, options: &Options) -> String {
    if result.columns.is_empty() {
        return String::new();
    }

    let cells: Vec<Vec<String>> = result.rows.iter().map(|row| texts(row)).collect();
    let widths = widths(&result.columns, &cells);

    let rule = |edge: char| {
        let mut line = String::from("+");
        for width in &widths {
            for _ in 0..width + 2 {
                line.push(edge);
            }
            line.push('+');
        }
        line.push('\n');
        line
    };
    let line = |values: &[String]| {
        let mut out = String::from("|");
        for (value, width) in values.iter().zip(&widths) {
            out.push(' ');
            out.push_str(&pad(&cut(value, *width), *width));
            out.push_str(" |");
        }
        out.push('\n');
        out
    };

    let mut out = rule('-');
    if options.header {
        let names: Vec<String> = result.columns.iter().map(|c| c.name.clone()).collect();
        out.push_str(&line(&names));
        out.push_str(&rule('='));
    }
    for row in &cells {
        out.push_str(&line(row));
    }
    out.push_str(&rule('-'));

    if options.footer {
        let _ = writeln!(out, "({})", summary(result));
    }
    out
}

fn markdown(result: &ResultSet) -> String {
    if result.columns.is_empty() {
        return String::new();
    }

    let mut out = String::from("|");
    for column in &result.columns {
        let _ = write!(out, " {} |", escape_pipes(&column.name));
    }
    out.push_str("\n|");
    for _ in &result.columns {
        out.push_str(" --- |");
    }
    out.push('\n');

    for row in &result.rows {
        out.push('|');
        for value in texts(row) {
            let _ = write!(out, " {} |", escape_pipes(&value));
        }
        out.push('\n');
    }
    out
}

fn vertical(result: &ResultSet, options: &Options) -> String {
    let width = result
        .columns
        .iter()
        .map(|column| column.name.width())
        .max()
        .unwrap_or(0);

    let mut out = String::new();
    for (index, row) in result.rows.iter().enumerate() {
        let _ = writeln!(
            out,
            "*************************** {}. row ***************************",
            index + 1
        );
        for (column, value) in result.columns.iter().zip(texts(row)) {
            let indent = " ".repeat(width.saturating_sub(column.name.width()));
            let _ = writeln!(out, "{indent}{}: {value}", column.name);
        }
    }
    if options.footer {
        let _ = writeln!(out, "({})", summary(result));
    }
    out
}

fn raw(result: &ResultSet) -> String {
    let mut out = String::new();
    for row in &result.rows {
        let _ = writeln!(out, "{}", texts(row).join("\t"));
    }
    out
}

fn separated(result: &ResultSet, delimiter: char, options: &Options) -> String {
    let mut out = String::new();

    if options.header && !result.columns.is_empty() {
        let names: Vec<String> = result
            .columns
            .iter()
            .map(|column| quote(&column.name, delimiter))
            .collect();
        let _ = writeln!(out, "{}", names.join(&delimiter.to_string()));
    }

    for row in &result.rows {
        // A NULL is an empty field here rather than the word: the file formats
        // have no way to tell one from the other, and inventing "NULL" would
        // make a string that reads NULL indistinguishable from the real thing.
        let fields: Vec<String> = row
            .iter()
            .map(|value| quote(&value.to_text(), delimiter))
            .collect();
        let _ = writeln!(out, "{}", fields.join(&delimiter.to_string()));
    }

    out
}

fn json_object(result: &ResultSet, options: &Options) -> String {
    let mut report = Map::new();
    report.insert(
        "columns".into(),
        Json::Array(
            result
                .columns
                .iter()
                .map(|column| {
                    json!({
                        "name": column.name,
                        "type": column.type_name,
                    })
                })
                .collect(),
        ),
    );
    report.insert("rows".into(), Json::Array(row_objects(result)));
    report.insert("row_count".into(), json!(result.rows.len()));
    report.insert("truncated".into(), json!(result.truncated));
    report.insert("duration_ms".into(), json!(milliseconds(result)));
    if let Some(affected) = result.rows_affected {
        report.insert("rows_affected".into(), json!(affected));
    }

    encode(&Json::Object(report), options.pretty)
}

fn jsonl(result: &ResultSet) -> String {
    let mut out = String::new();
    for row in row_objects(result) {
        let _ = writeln!(out, "{}", encode(&row, false).trim_end());
    }
    out
}

// --- helpers -------------------------------------------------------------

fn row_objects(result: &ResultSet) -> Vec<Json> {
    result
        .rows
        .iter()
        .map(|row| {
            let mut object = Map::new();
            for (index, column) in result.columns.iter().enumerate() {
                let value = row.get(index).map(json_value).unwrap_or(Json::Null);
                let key = key(&object, column, index);
                object.insert(key, value);
            }
            Json::Object(object)
        })
        .collect()
}

/// The column's name, made unique. Two columns of the same name are legal SQL
/// and a JSON object cannot hold both, so the later one is suffixed rather than
/// dropped on the floor.
fn key(taken: &Map<String, Json>, column: &Column, index: usize) -> String {
    let base = if column.name.is_empty() {
        format!("column_{}", index + 1)
    } else {
        column.name.clone()
    };
    if !taken.contains_key(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}:{n}"))
        .find(|candidate| !taken.contains_key(candidate))
        .unwrap_or(base)
}

fn json_value(value: &Value) -> Json {
    match value {
        Value::Null => Json::Null,
        Value::Bool(b) => json!(b),
        Value::Int(i) => json!(i),
        Value::Float(f) => Number::from_f64(*f).map(Json::Number).unwrap_or(Json::Null),
        // Kept as text for the same reason the core does: no JSON number
        // round-trips every backend's NUMERIC.
        Value::Decimal(s) => json!(s),
        Value::Text(s) | Value::Uuid(s) | Value::Timestamp(s) => json!(s),
        // Already JSON on the way in, so it goes back as JSON rather than as a
        // string holding a document.
        Value::Json(s) => serde_json::from_str(s).unwrap_or_else(|_| json!(s)),
        Value::Bytes(_) => json!(value.to_text()),
    }
}

fn encode(value: &Json, pretty: bool) -> String {
    let encoded = if pretty {
        serde_json::to_string_pretty(value)
    } else {
        serde_json::to_string(value)
    };
    // The only way this fails is a non-string map key, which none of these
    // objects has.
    let mut encoded = encoded.unwrap_or_default();
    encoded.push('\n');
    encoded
}

fn texts(row: &[Value]) -> Vec<String> {
    row.iter()
        .map(|value| {
            if value.is_null() {
                NULL.to_string()
            } else {
                value.to_text()
            }
        })
        .collect()
}

fn widths(columns: &[Column], rows: &[Vec<String>]) -> Vec<usize> {
    let mut widths: Vec<usize> = columns
        .iter()
        .map(|column| column.name.width().min(MAX_COLUMN_WIDTH))
        .collect();

    for row in rows {
        for (index, value) in row.iter().enumerate() {
            if let Some(width) = widths.get_mut(index) {
                *width = (*width).max(value.width().min(MAX_COLUMN_WIDTH));
            }
        }
    }
    widths
}

fn summary(result: &ResultSet) -> String {
    let count = result.rows.len();
    let noun = if count == 1 { "row" } else { "rows" };
    let more = if result.truncated {
        ", more available"
    } else {
        ""
    };
    format!("{count} {noun}{more}, {:.1}ms", milliseconds(result))
}

fn milliseconds(result: &ResultSet) -> f64 {
    result.elapsed.as_secs_f64() * 1000.0
}

fn escape_pipes(value: &str) -> String {
    value.replace('|', "\\|")
}

/// Quotes a field for CSV or TSV. Newlines are kept verbatim inside the quotes,
/// which is what the format is for.
fn quote(value: &str, delimiter: char) -> String {
    let needs_quotes = value.contains(delimiter)
        || value.contains('"')
        || value.contains('\n')
        || value.contains('\r');
    if !needs_quotes {
        return value.to_string();
    }
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn pad(value: &str, width: usize) -> String {
    let mut out = value.to_string();
    for _ in value.width()..width {
        out.push(' ');
    }
    out
}

/// The longest prefix that fits, with an ellipsis when something was left out.
fn cut(value: &str, width: usize) -> String {
    if value.width() <= width {
        return value.to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    for ch in value.chars() {
        let next = ch.to_string().width();
        if used + next > width.saturating_sub(1) {
            break;
        }
        out.push(ch);
        used += next;
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn result() -> ResultSet {
        ResultSet {
            columns: vec![Column::new("id", "int"), Column::new("name", "text")],
            rows: vec![
                vec![Value::Int(1), Value::Text("Portishead".into())],
                vec![Value::Int(2), Value::Null],
            ],
            rows_affected: None,
            elapsed: Duration::from_micros(1500),
            truncated: false,
        }
    }

    fn options(format: Format) -> Options {
        Options {
            format,
            ..Options::default()
        }
    }

    #[test]
    fn table_draws_a_header_and_a_count() {
        let out = rows(&result(), &options(Format::Table));
        assert!(out.contains("| id | name       |"), "{out}");
        assert!(out.contains("| 1  | Portishead |"), "{out}");
        assert!(out.contains("NULL"), "{out}");
        assert!(out.contains("(2 rows"), "{out}");
    }

    #[test]
    fn csv_quotes_only_what_needs_it_and_leaves_null_empty() {
        let mut quoted = result();
        quoted.rows[1][1] = Value::Text("a,b \"quoted\"".into());
        let out = rows(&quoted, &options(Format::Csv));
        assert_eq!(
            out, "id,name\n1,Portishead\n2,\"a,b \"\"quoted\"\"\"\n",
            "{out}"
        );

        let out = rows(&result(), &options(Format::Csv));
        assert_eq!(out, "id,name\n1,Portishead\n2,\n", "{out}");
    }

    #[test]
    fn json_carries_types_and_a_null() {
        let out = rows(&result(), &options(Format::Json));
        let parsed: Json = serde_json::from_str(&out).expect("valid json");
        assert_eq!(parsed["rows"][0]["id"], json!(1));
        assert_eq!(parsed["rows"][0]["name"], json!("Portishead"));
        assert_eq!(parsed["rows"][1]["name"], Json::Null);
        assert_eq!(parsed["row_count"], json!(2));
        assert_eq!(parsed["columns"][0]["type"], json!("int"));
    }

    #[test]
    fn jsonl_is_one_object_per_line() {
        let out = rows(&result(), &options(Format::Jsonl));
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2, "{out}");
        for line in lines {
            serde_json::from_str::<Json>(line).expect("each line is json");
        }
    }

    #[test]
    fn markdown_escapes_a_pipe() {
        let mut result = result();
        result.rows[1][1] = Value::Text("a|b".into());
        let out = rows(&result, &options(Format::Markdown));
        assert!(out.contains("a\\|b"), "{out}");
        assert!(out.contains("| --- | --- |"), "{out}");
    }

    #[test]
    fn none_prints_nothing() {
        assert_eq!(rows(&result(), &options(Format::None)), "");
    }

    #[test]
    fn a_duplicate_column_name_keeps_both_values() {
        let result = ResultSet {
            columns: vec![Column::new("id", "int"), Column::new("id", "int")],
            rows: vec![vec![Value::Int(1), Value::Int(2)]],
            rows_affected: None,
            elapsed: Duration::ZERO,
            truncated: false,
        };
        let out = rows(&result, &options(Format::Json));
        let parsed: Json = serde_json::from_str(&out).expect("valid json");
        assert_eq!(parsed["rows"][0]["id"], json!(1));
        assert_eq!(parsed["rows"][0]["id:2"], json!(2));
    }

    #[test]
    fn a_wide_value_is_cut_to_the_column() {
        let long = "x".repeat(MAX_COLUMN_WIDTH * 2);
        let result = ResultSet {
            columns: vec![Column::new("wide", "text")],
            rows: vec![vec![Value::Text(long)]],
            rows_affected: None,
            elapsed: Duration::ZERO,
            truncated: false,
        };
        let out = rows(&result, &options(Format::Table));
        assert!(out.contains('…'), "{out}");
        assert!(
            out.lines().all(|line| line.width() <= MAX_COLUMN_WIDTH + 4),
            "{out}"
        );
    }
}
