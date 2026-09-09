//! A query console: one editor, one result, bound to one data source.
//!
//! Consoles are the tabs across the top of the right-hand pane. Each keeps its
//! own connection binding, so a session can hold a scratch query against
//! `local` open next to a report running on `prod` without either losing its
//! place.

use std::time::Duration;

use binsql_core::{ResultSet, Value};
use tui_textarea::TextArea;
use unicode_width::UnicodeWidthStr;

use crate::theme;

/// Rows a console pulls for one run. High enough to scroll through, low enough
/// that a mistyped `SELECT *` on a huge table is not a wait.
pub const DEFAULT_LIMIT: usize = 500;

const MIN_COLUMN_WIDTH: u16 = 4;
const MAX_COLUMN_WIDTH: u16 = 48;
/// Rows sampled when sizing columns. Measuring every row of a large result to
/// pick a width costs more than the width is worth.
const WIDTH_SAMPLE: usize = 200;

pub enum Outcome {
    /// Nothing has been run in this console yet.
    Idle,
    Running,
    Rows(Grid),
    Affected {
        count: u64,
        elapsed: Duration,
    },
    Error(String),
}

pub struct Console {
    /// Identifies the console across tab closes, so a result can only land in
    /// the console that asked for it.
    pub id: u64,
    pub title: String,
    /// The data source this console runs against; `None` until one is bound.
    pub source: Option<String>,
    pub catalog: Option<String>,
    pub editor: TextArea<'static>,
    pub outcome: Outcome,
    /// Bumped on every run so a result that arrives after a newer one has
    /// started is discarded instead of overwriting it.
    pub generation: u64,
}

/// Keywords lit up in the editor.
///
/// tui-textarea has no per-token styling hook, but it does highlight every
/// match of a search pattern — so the pattern is the keyword list and the
/// search style is the keyword style. The trade is that interactive search is
/// spoken for; nothing binds it, and coloured keywords are worth more.
const KEYWORDS: &str = r"(?i)\b(select|from|where|join|inner|left|right|full|outer|cross|on|group|order|by|having|limit|offset|top|distinct|as|and|or|not|in|is|null|like|between|exists|case|when|then|else|end|union|all|except|intersect|with|insert|into|values|update|set|delete|create|alter|drop|truncate|table|view|index|primary|foreign|key|references|constraint|default|asc|desc|count|sum|avg|min|max|coalesce|cast|over|partition)\b";

fn new_editor() -> TextArea<'static> {
    let mut editor = TextArea::default();
    editor.set_cursor_line_style(theme::sql_text());
    editor.set_selection_style(theme::selection(true));
    editor.set_placeholder_text("Write SQL here — ⌃R runs it");
    editor.set_placeholder_style(theme::dim());
    editor.set_search_style(theme::sql_keyword());
    let _ = editor.set_search_pattern(KEYWORDS);
    editor
}

impl Console {
    pub fn new(title: impl Into<String>) -> Console {
        let editor = new_editor();

        Console {
            id: 0,
            title: title.into(),
            source: None,
            catalog: None,
            editor,
            outcome: Outcome::Idle,
            generation: 0,
        }
    }

    pub fn sql(&self) -> String {
        self.editor.lines().join("\n")
    }

    /// What ⌃R runs: the selection if there is one, otherwise the whole buffer.
    /// Matches how a console is actually used — a scratchpad of several
    /// statements, one of which you want now.
    pub fn sql_to_run(&self) -> String {
        self.selected_text()
            .filter(|text| !text.trim().is_empty())
            .unwrap_or_else(|| self.sql())
    }

    /// The selected text, assembled from the buffer. tui-textarea reports the
    /// selection as row/column bounds and only hands over the text through its
    /// yank buffer, which would mean mutating the editor to read it.
    fn selected_text(&self) -> Option<String> {
        let (start, end) = self.editor.selection_range()?;
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        let ((start_row, start_col), (end_row, end_col)) = (start, end);
        let lines = self.editor.lines();

        if start_row == end_row {
            let line = lines.get(start_row)?;
            return Some(
                line.chars()
                    .skip(start_col)
                    .take(end_col.saturating_sub(start_col))
                    .collect(),
            );
        }

        let mut out: String = lines.get(start_row)?.chars().skip(start_col).collect();
        for row in start_row + 1..end_row {
            out.push('\n');
            out.push_str(lines.get(row)?);
        }
        out.push('\n');
        out.extend(lines.get(end_row)?.chars().take(end_col));
        Some(out)
    }

    pub fn set_sql(&mut self, sql: &str) {
        let mut editor = new_editor();
        editor.insert_str(sql);
        editor.move_cursor(tui_textarea::CursorMove::End);
        self.editor = editor;
    }

    pub fn grid(&self) -> Option<&Grid> {
        match &self.outcome {
            Outcome::Rows(grid) => Some(grid),
            _ => None,
        }
    }

    pub fn grid_mut(&mut self) -> Option<&mut Grid> {
        match &mut self.outcome {
            Outcome::Rows(grid) => Some(grid),
            _ => None,
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(self.outcome, Outcome::Running)
    }
}

/// A result set plus where the cursor is in it.
pub struct Grid {
    pub result: ResultSet,
    pub row: usize,
    pub column: usize,
    pub row_offset: usize,
    pub column_offset: usize,
    /// Rendered width per column, measured once when the result arrives.
    pub widths: Vec<u16>,
    /// Whether each column right-aligns. Decided per column rather than per
    /// cell, so a NULL in a number column does not break the column of digits.
    pub numeric: Vec<bool>,
}

impl Grid {
    pub fn new(result: ResultSet) -> Grid {
        let (widths, numeric) = measure(&result);
        Grid {
            result,
            row: 0,
            column: 0,
            row_offset: 0,
            column_offset: 0,
            widths,
            numeric,
        }
    }

    pub fn rows(&self) -> usize {
        self.result.rows.len()
    }

    pub fn columns(&self) -> usize {
        self.result.columns.len()
    }

    pub fn selected_value(&self) -> Option<&Value> {
        self.result.rows.get(self.row)?.get(self.column)
    }

    pub fn move_row(&mut self, delta: isize) {
        self.row = clamp_step(self.row, delta, self.rows());
    }

    pub fn move_column(&mut self, delta: isize) {
        self.column = clamp_step(self.column, delta, self.columns());
    }

    pub fn first_row(&mut self) {
        self.row = 0;
    }

    pub fn last_row(&mut self) {
        self.row = self.rows().saturating_sub(1);
    }

    pub fn first_column(&mut self) {
        self.column = 0;
    }

    pub fn last_column(&mut self) {
        self.column = self.columns().saturating_sub(1);
    }

    /// Keeps the cursor inside the visible window, scrolling the minimum
    /// needed. Called at render time, when the viewport size is known.
    pub fn scroll_into_view(&mut self, visible_rows: usize, visible_width: u16) {
        if self.row < self.row_offset {
            self.row_offset = self.row;
        } else if visible_rows > 0 && self.row >= self.row_offset + visible_rows {
            self.row_offset = self.row + 1 - visible_rows;
        }

        if self.column < self.column_offset {
            self.column_offset = self.column;
            return;
        }
        // Widen the window leftwards until the selected column fits.
        while self.column_offset < self.column {
            let used: u16 = (self.column_offset..=self.column)
                .map(|i| self.widths.get(i).copied().unwrap_or(0) + 1)
                .sum();
            if used <= visible_width {
                break;
            }
            self.column_offset += 1;
        }
    }
}

fn clamp_step(current: usize, delta: isize, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    let next = current as isize + delta;
    next.clamp(0, count as isize - 1) as usize
}

fn measure(result: &ResultSet) -> (Vec<u16>, Vec<bool>) {
    result
        .columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let header = UnicodeWidthStr::width(column.name.as_str());
            let sample = result
                .rows
                .iter()
                .take(WIDTH_SAMPLE)
                .filter_map(|row| row.get(index));

            let mut widest = 0;
            let mut numeric = false;
            let mut seen_value = false;
            for value in sample {
                widest = widest.max(UnicodeWidthStr::width(display(value).as_str()));
                if !value.is_null() {
                    numeric = if seen_value {
                        numeric && value.is_numeric()
                    } else {
                        value.is_numeric()
                    };
                    seen_value = true;
                }
            }

            let width = (header.max(widest) as u16).clamp(MIN_COLUMN_WIDTH, MAX_COLUMN_WIDTH);
            (width, numeric)
        })
        .unzip()
}

/// One cell as a single line. Embedded newlines and tabs would break the grid,
/// so they are shown as glyphs; the detail overlay has the real text.
pub fn display(value: &Value) -> String {
    match value {
        Value::Null => "NULL".to_string(),
        other => {
            let text = other.to_text();
            if text.contains(['\n', '\r', '\t']) {
                text.replace('\r', "").replace('\n', "␤").replace('\t', "␉")
            } else {
                text
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use binsql_core::Column;

    fn grid_of(columns: &[&str], rows: Vec<Vec<Value>>) -> Grid {
        Grid::new(ResultSet {
            columns: columns.iter().map(|c| Column::new(*c, "text")).collect(),
            rows,
            rows_affected: None,
            elapsed: Duration::ZERO,
            truncated: false,
        })
    }

    #[test]
    fn sizes_columns_to_content_within_bounds() {
        let grid = grid_of(
            &["id", "name"],
            vec![vec![Value::Int(1), Value::Text("a".repeat(100))]],
        );
        // "id" is shorter than the floor, the long name is past the ceiling.
        assert_eq!(grid.widths[0], MIN_COLUMN_WIDTH);
        assert_eq!(grid.widths[1], MAX_COLUMN_WIDTH);
    }

    #[test]
    fn navigation_stops_at_the_edges() {
        let mut grid = grid_of(&["a", "b"], vec![vec![Value::Int(1), Value::Int(2)]]);
        grid.move_row(-1);
        assert_eq!(grid.row, 0);
        grid.move_column(5);
        assert_eq!(grid.column, 1);
    }

    #[test]
    fn scrolling_follows_the_cursor() {
        let rows: Vec<Vec<Value>> = (0..50).map(|i| vec![Value::Int(i)]).collect();
        let mut grid = grid_of(&["n"], rows);
        grid.row = 30;
        grid.scroll_into_view(10, 20);
        assert_eq!(grid.row_offset, 21);

        grid.row = 2;
        grid.scroll_into_view(10, 20);
        assert_eq!(grid.row_offset, 2);
    }

    #[test]
    fn renders_newlines_as_glyphs() {
        assert_eq!(display(&Value::Text("a\nb".into())), "a␤b");
        assert_eq!(display(&Value::Null), "NULL");
    }
}
