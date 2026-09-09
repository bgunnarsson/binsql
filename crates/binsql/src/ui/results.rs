use binsql_core::Value;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::console::{Outcome, display};
use crate::app::{App, Pane, format_elapsed};
use crate::theme;
use crate::ui;

/// Space between columns, and the width of the row-number gutter's separator.
const GAP: u16 = 1;

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Pane::Results;
    let (title, counter) = title(app);
    let block = ui::body_pane(&title, counter, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    match &app.consoles[app.active_console].outcome {
        Outcome::Idle => {
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("⌃R", theme::key()),
                    Span::styled(" runs the query · ", theme::dim()),
                    Span::styled("⌃K", theme::key()),
                    Span::styled(" for commands", theme::dim()),
                ])),
                inner,
            );
            return;
        }
        Outcome::Running => {
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("Running… ", theme::warning()),
                    Span::styled("⌃C", theme::key()),
                    Span::styled(" cancels", theme::dim()),
                ])),
                inner,
            );
            return;
        }
        Outcome::Cancelled => {
            frame.render_widget(
                Paragraph::new(Span::styled("Cancelled", theme::muted())),
                inner,
            );
            return;
        }
        Outcome::Error(error) => {
            frame.render_widget(
                Paragraph::new(Span::styled(error.clone(), theme::danger()))
                    .wrap(ratatui::widgets::Wrap { trim: false }),
                inner,
            );
            return;
        }
        Outcome::Affected { count, elapsed } => {
            let rows = if *count == 1 { "row" } else { "rows" };
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(count.to_string(), theme::success()),
                    Span::styled(format!(" {rows} affected in "), theme::muted()),
                    Span::styled(format_elapsed(*elapsed), theme::muted()),
                ])),
                inner,
            );
            return;
        }
        Outcome::Rows(_) => {}
    }

    draw_grid(frame, app, inner, focused);
}

fn draw_grid(frame: &mut Frame, app: &mut App, area: Rect, focused: bool) {
    if area.height < 2 {
        return;
    }

    // The row-number gutter is sized to the largest number it will show.
    let console = &mut app.consoles[app.active_console];
    let Some(grid) = console.grid_mut() else {
        return;
    };
    let gutter = (grid.rows().to_string().len() as u16).max(3) + GAP;
    let body_width = area.width.saturating_sub(gutter);
    let visible_rows = area.height.saturating_sub(1) as usize;

    grid.scroll_into_view(visible_rows, body_width);

    let columns: Vec<usize> = (grid.column_offset..grid.columns())
        .scan(0u16, |used, index| {
            let width = grid.widths[index] + GAP;
            if *used + width > body_width && index > grid.column_offset {
                return None;
            }
            *used += width;
            Some(index)
        })
        .collect();

    let mut lines = Vec::with_capacity(visible_rows + 1);

    let mut header = vec![Span::styled(" ".repeat(gutter as usize), theme::header())];
    for index in &columns {
        let column = &grid.result.columns[*index];
        let width = grid.widths[*index] as usize;
        header.push(Span::styled(
            pad(
                &ui::truncate(&column.name, width),
                width,
                grid.numeric[*index],
            ),
            theme::header(),
        ));
        header.push(Span::styled(" ", theme::header()));
    }
    lines.push(Line::from(header));

    for row_index in grid.row_offset..(grid.row_offset + visible_rows).min(grid.rows()) {
        let is_current_row = row_index == grid.row;
        let mut spans = vec![Span::styled(
            pad(&(row_index + 1).to_string(), gutter as usize - 1, true),
            if is_current_row {
                theme::muted()
            } else {
                theme::dim()
            },
        )];
        spans.push(Span::raw(" "));

        for index in &columns {
            let value = &grid.result.rows[row_index][*index];
            let width = grid.widths[*index] as usize;
            let text = pad(
                &ui::truncate(&display(value), width),
                width,
                grid.numeric[*index],
            );

            // The cell keeps its own value colour under the selection; only
            // the background says where the cursor is.
            let mut style = value_style(value);
            if is_current_row && *index == grid.column {
                style = style.bg(theme::cell_cursor(focused));
            } else if is_current_row {
                style = style.bg(theme::row_cursor());
            }

            spans.push(Span::styled(text, style));
            spans.push(Span::styled(
                " ",
                if is_current_row {
                    Style::default().bg(theme::row_cursor())
                } else {
                    Style::default()
                },
            ));
        }
        lines.push(Line::from(spans));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

/// Shared with the row-detail overlay so a value is the same colour
/// wherever it is shown.
pub(crate) fn value_style(value: &Value) -> Style {
    match value {
        Value::Null => theme::cell_null(),
        Value::Bool(_) => theme::cell_bool(),
        Value::Int(_) | Value::Float(_) | Value::Decimal(_) => theme::cell_number(),
        Value::Timestamp(_) => theme::cell_temporal(),
        _ => theme::cell_text(),
    }
}

fn pad(text: &str, width: usize, right_align: bool) -> String {
    let used = unicode_width::UnicodeWidthStr::width(text);
    let fill = width.saturating_sub(used);
    if right_align {
        format!("{}{}", " ".repeat(fill), text)
    } else {
        format!("{}{}", text, " ".repeat(fill))
    }
}

/// The pane's title and the counter that sits at the right end of its border.
/// Splitting them keeps the left end stable while the cursor moves.
fn title(app: &App) -> (String, Option<String>) {
    let console = app.console();
    let Some(grid) = console.grid() else {
        return ("Results".to_string(), None);
    };

    let column = grid
        .result
        .columns
        .get(grid.column)
        .map(|c| format!(" · {} {}", c.name, c.type_name))
        .unwrap_or_default();
    let more = if grid.result.truncated {
        " · more"
    } else {
        ""
    };

    (
        format!(
            "Results · {} in {}{more}{column}",
            plural(grid.rows(), "row"),
            format_elapsed(grid.result.elapsed)
        ),
        Some(format!("{}/{}", grid.row + 1, grid.rows())),
    )
}

fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("{count} {noun}")
    } else {
        format!("{count} {noun}s")
    }
}
