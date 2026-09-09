use std::fmt;
use std::time::Duration;

/// One cell. Values arrive from four very different drivers, so they are
/// normalised into this set on the way out of the adapter — the UI and any
/// future output formatter never see a driver type.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    /// Kept as text because neither `f64` nor a fixed-precision type round-trips
    /// every backend's NUMERIC faithfully.
    Decimal(String),
    Text(String),
    Bytes(Vec<u8>),
    Uuid(String),
    Json(String),
    Timestamp(String),
}

impl Value {
    /// True for values a grid should right-align.
    pub fn is_numeric(&self) -> bool {
        matches!(self, Value::Int(_) | Value::Float(_) | Value::Decimal(_))
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// The full value as text. `NULL` renders as an empty string here; the grid
    /// draws its own dimmed marker so a real empty string stays distinguishable.
    pub fn to_text(&self) -> String {
        match self {
            Value::Null => String::new(),
            Value::Bool(b) => b.to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => format_float(*f),
            Value::Decimal(s) | Value::Text(s) | Value::Uuid(s) | Value::Json(s) => s.clone(),
            Value::Timestamp(s) => s.clone(),
            Value::Bytes(b) => format_bytes(b),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_text())
    }
}

fn format_float(f: f64) -> String {
    if f.fract() == 0.0 && f.abs() < 1e15 {
        format!("{f:.1}")
    } else {
        f.to_string()
    }
}

/// Binary is shown as a hex preview — a blob dumped raw into a terminal grid
/// corrupts the display, and the row-detail overlay wants the same rendering.
fn format_bytes(bytes: &[u8]) -> String {
    const PREVIEW: usize = 32;
    let mut out = String::from("0x");
    for byte in bytes.iter().take(PREVIEW) {
        out.push_str(&format!("{byte:02X}"));
    }
    if bytes.len() > PREVIEW {
        out.push_str(&format!("… ({} bytes)", bytes.len()));
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    /// The backend's own type name, shown verbatim — `int4` and `INT` are
    /// meaningful to someone reading a Postgres or MySQL schema.
    pub type_name: String,
    pub nullable: Option<bool>,
    pub default: Option<String>,
    pub primary_key: bool,
}

impl Column {
    pub fn new(name: impl Into<String>, type_name: impl Into<String>) -> Self {
        Column {
            name: name.into(),
            type_name: type_name.into(),
            nullable: None,
            default: None,
            primary_key: false,
        }
    }
}

/// What a statement produced. A `SELECT` yields rows; an `INSERT` yields a
/// count. Both carry timing, because the first thing you want after a slow
/// query is how slow.
#[derive(Debug, Clone)]
pub struct ResultSet {
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<Value>>,
    pub rows_affected: Option<u64>,
    pub elapsed: Duration,
    /// Set when the adapter stopped fetching at the row limit, so the UI can
    /// say "first 500" rather than implying the table has 500 rows.
    pub truncated: bool,
}

impl ResultSet {
    pub fn empty() -> Self {
        ResultSet {
            columns: Vec::new(),
            rows: Vec::new(),
            rows_affected: None,
            elapsed: Duration::ZERO,
            truncated: false,
        }
    }

    pub fn affected(count: u64, elapsed: Duration) -> Self {
        ResultSet {
            columns: Vec::new(),
            rows: Vec::new(),
            rows_affected: Some(count),
            elapsed,
            truncated: false,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.columns.is_empty() && self.rows.is_empty()
    }
}
