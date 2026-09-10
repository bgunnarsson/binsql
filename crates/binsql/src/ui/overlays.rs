use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};

use crate::app::App;
use crate::app::overlay::{ConnectForm, Field, Overlay, Palette};
use crate::theme;
use crate::ui;
use crate::ui::results;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    match &app.overlay {
        None => {}
        Some(Overlay::Splash) => splash(frame, app, area),
        Some(Overlay::Help) => help(frame, area),
        // Takes `app` mutably: the renderer is what discovers how tall the
        // record is, and the scroll limit follows from that.
        Some(Overlay::Detail(_)) => detail(frame, app, area),
        Some(Overlay::Value(_)) => value(frame, app, area),
        Some(Overlay::Palette(palette)) => command_palette(frame, palette, area),
        Some(Overlay::Connect(form)) => connect(frame, form, area),
    }
}

fn frame_for(frame: &mut Frame, area: Rect, title: &str) -> Rect {
    frame.render_widget(Clear, area);
    let block = ui::pane(title, true).style(theme::overlay());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

fn frame_for_counted(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    counter: impl Into<String>,
) -> Rect {
    frame.render_widget(Clear, area);
    let block = ui::counted_pane(title, counter, true).style(theme::overlay());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

/// The wordmark, in the ANSI-shadow shape a terminal splash is expected to
/// wear. Every glyph here is single-width, so the block is exactly as wide as
/// it looks.
const WORDMARK: [&str; 6] = [
    "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557}  \u{2588}\u{2588}\u{2557} \u{2588}\u{2588}\u{2588}\u{2557}   \u{2588}\u{2588}\u{2557} \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557}  \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557}  \u{2588}\u{2588}\u{2557}     ",
    "\u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2588}\u{2588}\u{2557} \u{2588}\u{2588}\u{2551} \u{2588}\u{2588}\u{2588}\u{2588}\u{2557}  \u{2588}\u{2588}\u{2551} \u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2550}\u{2550}\u{255d} \u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2550}\u{2588}\u{2588}\u{2557} \u{2588}\u{2588}\u{2551}     ",
    "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2554}\u{255d} \u{2588}\u{2588}\u{2551} \u{2588}\u{2588}\u{2554}\u{2588}\u{2588}\u{2557} \u{2588}\u{2588}\u{2551} \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557} \u{2588}\u{2588}\u{2551}   \u{2588}\u{2588}\u{2551} \u{2588}\u{2588}\u{2551}     ",
    "\u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2588}\u{2588}\u{2557} \u{2588}\u{2588}\u{2551} \u{2588}\u{2588}\u{2551}\u{255a}\u{2588}\u{2588}\u{2557}\u{2588}\u{2588}\u{2551} \u{255a}\u{2550}\u{2550}\u{2550}\u{2550}\u{2588}\u{2588}\u{2551} \u{2588}\u{2588}\u{2551}\u{2584}\u{2584} \u{2588}\u{2588}\u{2551} \u{2588}\u{2588}\u{2551}     ",
    "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2554}\u{255d} \u{2588}\u{2588}\u{2551} \u{2588}\u{2588}\u{2551} \u{255a}\u{2588}\u{2588}\u{2588}\u{2588}\u{2551} \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2551} \u{255a}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2554}\u{255d} \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557}",
    "\u{255a}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{255d}  \u{255a}\u{2550}\u{255d} \u{255a}\u{2550}\u{255d}  \u{255a}\u{2550}\u{2550}\u{2550}\u{255d} \u{255a}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{255d}  \u{255a}\u{2550}\u{2550}\u{2580}\u{2580}\u{2550}\u{255d}  \u{255a}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{255d}",
];

/// The greeting, shown once at startup.
///
/// Sized to the wordmark, and falls back to plain text in a terminal too narrow
/// to hold it — a splash that overflows its own box is worse than no splash.
fn splash(frame: &mut Frame, app: &App, area: Rect) {
    let wordmark_width = WORDMARK
        .iter()
        .map(|row| UnicodeWidthStr::width(*row))
        .max()
        .unwrap_or(0);
    let roomy = area.width as usize >= wordmark_width + 8;

    let mut lines: Vec<Line<'static>> = Vec::new();
    if roomy {
        for row in WORDMARK {
            lines.push(Line::from(Span::styled(row, theme::brand())));
        }
    } else {
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", theme::MARK), theme::brand()),
            Span::styled("binsql", theme::title(true)),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "a database IDE for the terminal",
        theme::muted(),
    )));
    lines.push(Line::from(""));

    let registered = app.config.len();
    lines.push(Line::from(Span::styled(
        match registered {
            0 => "No data sources yet".to_string(),
            1 => "1 data source registered".to_string(),
            n => format!("{n} data sources registered"),
        },
        theme::muted(),
    )));
    lines.push(Line::from(""));

    for (binding, what) in [
        ("⌃N", "add a data source"),
        ("⌃K", "commands"),
        ("F1", "help"),
    ] {
        lines.push(Line::from(vec![
            Span::styled(format!("{binding:<4}"), theme::key()),
            Span::styled(what, theme::muted()),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("Any key to begin.", theme::dim())));

    let content_width = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
                .sum::<usize>()
        })
        .max()
        .unwrap_or(0);

    let width = saturating_u16(content_width + BORDERS as usize + PADDING * 2);
    let height = saturating_u16(lines.len() + BORDERS as usize);
    let inner = frame_for_counted(
        frame,
        ui::centered_size(area, width, height),
        "binsql",
        format!("v{}", env!("CARGO_PKG_VERSION")),
    );

    frame.render_widget(
        Paragraph::new(lines),
        Rect {
            x: inner.x + PADDING as u16,
            width: inner.width.saturating_sub(PADDING as u16 * 2),
            ..inner
        },
    );
}

/// Breathing room either side of an overlay's contents.
const PADDING: usize = 2;

/// Renders one help section per heading, blank-separated, with no trailing gap.
fn section_lines(sections: &[Section]) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for (heading, bindings) in sections {
        lines.push(Line::from(Span::styled(
            (*heading).to_string(),
            theme::title(true),
        )));
        for (keys, description) in *bindings {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{keys:<13}"), theme::key()),
                Span::styled(*description, theme::muted()),
            ]));
        }
        lines.push(Line::from(""));
    }
    lines.pop();
    lines
}

/// The widest line in a block, in display columns — what a box has to be to
/// hold it without cutting anything off.
fn block_width(lines: &[Line<'_>]) -> usize {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
                .sum::<usize>()
        })
        .max()
        .unwrap_or(0)
}

fn help(frame: &mut Frame, area: Rect) {
    // Two columns: the bindings do not fit down one, and a help screen that
    // scrolls is a help screen nobody reads to the end of. Split where the two
    // columns come out closest to level.
    let left = section_lines(&SECTIONS[..LEFT_COLUMN]);
    let right = section_lines(&SECTIONS[LEFT_COLUMN..]);

    const GUTTER: u16 = 4;
    let left_width = saturating_u16(block_width(&left));
    let right_width = saturating_u16(block_width(&right));
    let width = left_width + GUTTER + right_width + BORDERS + PADDING as u16 * 2;
    let height = saturating_u16(left.len().max(right.len()) + 1) + BORDERS;

    let inner = frame_for(frame, ui::centered_size(area, width, height), "Help");
    if inner.height < 3 {
        return;
    }

    let columns = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Horizontal)
        .constraints([
            ratatui::layout::Constraint::Length(left_width),
            ratatui::layout::Constraint::Length(GUTTER),
            ratatui::layout::Constraint::Min(0),
        ])
        .split(Rect {
            x: inner.x + PADDING as u16,
            width: inner.width.saturating_sub(PADDING as u16 * 2),
            height: inner.height - 1,
            ..inner
        });

    frame.render_widget(Paragraph::new(left), columns[0]);
    frame.render_widget(Paragraph::new(right), columns[2]);

    frame.render_widget(
        Paragraph::new(Span::styled("Any key closes this.", theme::dim())),
        Rect {
            x: inner.x + PADDING as u16,
            y: inner.y + inner.height - 1,
            width: inner.width.saturating_sub(PADDING as u16 * 2),
            height: 1,
        },
    );
}

/// Narrow enough for a two-column record, wide enough for the modal's own
/// title and footer.
const MIN_DETAIL_WIDTH: u16 = 46;
/// The two edges of a bordered box, in either direction.
const BORDERS: u16 = 2;

type Section = (&'static str, &'static [(&'static str, &'static str)]);

/// How many sections the left-hand help column takes.
const LEFT_COLUMN: usize = 2;

const SECTIONS: &[Section] = &[
    (
        "Anywhere",
        &[
            ("⌃R", "Run the query or selection"),
            ("⌃C", "Cancel the running query"),
            ("⌃K", "Command palette"),
            ("⌃T / ⌃W", "New console / close console"),
            ("⌥1…9", "Jump to console"),
            ("⌃N", "New data source"),
            ("Tab / ⇧Tab", "Cycle panes"),
            ("⌥h ⌥k ⌥j", "Databases / query / results"),
            ("F1 or ?", "This help"),
            ("F5", "Refresh the selected node"),
            ("click", "Focus, and land the cursor"),
            ("drag an edge", "Resize the panes"),
            ("⌃Q", "Quit"),
        ],
    ),
    (
        "Record",
        &[
            ("j / k", "Move between fields"),
            ("Enter", "Open the value in full"),
            ("h / l", "Previous / next record"),
            ("Esc", "Close"),
        ],
    ),
    (
        "Databases",
        &[
            ("j / k", "Move, g / G for the ends"),
            ("l / Space", "Expand"),
            ("h", "Collapse, or go to parent"),
            ("Enter", "Connect, or open the table"),
            ("n / e / d", "New / edit / disconnect"),
            ("r", "Reload from the server"),
        ],
    ),
    (
        "Results",
        &[
            ("h j k l", "Move by cell"),
            ("⌃D / ⌃U", "Half page"),
            ("g / G", "First / last row"),
            ("0 / $", "First / last column"),
            ("Enter", "Open the whole record"),
        ],
    ),
    (
        "Query",
        &[
            ("⌃A", "Select all"),
            ("⌃U or ⌃⌫", "Delete to line start"),
            ("⌃Z", "Undo, a word at a time"),
            ("⌃⇧Z or ⌃Y", "Redo"),
            ("Esc / ⇧Tab", "Leave the editor"),
            ("Select text", "⌃R then runs only that"),
        ],
    ),
];

/// The selected row with every column stacked, label beside value.
///
/// The grid can only give a value one line and truncates to fit; this is where
/// a long comment or a wrapped connection string is actually readable.
///
/// The box is sized to the record, not to the screen. A table with four short
/// columns opens a small modal; a wide one grows until it hits the screen and
/// then scrolls.
fn detail(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(grid) = app.console().grid() else {
        let inner = frame_for(frame, ui::centered_size(area, 24, 3), "Row");
        frame.render_widget(
            Paragraph::new(Span::styled("No rows.", theme::dim())),
            inner,
        );
        return;
    };

    let Some(row) = grid.result.rows.get(grid.row) else {
        return;
    };

    // The focused column's type goes in the title rather than beside its
    // value, where it would sit as an orphan line in the middle of the record.
    let focused_column = grid
        .result
        .columns
        .get(grid.column)
        .map(|column| format!(" · {} {}", column.name, column.type_name))
        .unwrap_or_default();
    let title = format!("Row {} of {}{focused_column}", grid.row + 1, grid.rows());

    // The label column is as wide as the widest name, within reason — a table
    // with one very long column name should not squeeze every value.
    let label_width = grid
        .result
        .columns
        .iter()
        .map(|column| UnicodeWidthStr::width(column.name.as_str()))
        .max()
        .unwrap_or(0)
        .clamp(6, 28);
    let gap = 2;

    let widest_value = row
        .iter()
        .map(|value| UnicodeWidthStr::width(field_text(value).as_str()))
        .max()
        .unwrap_or(0);
    let ceiling = (area.width * 88 / 100).max(1);
    let floor = MIN_DETAIL_WIDTH.min(ceiling);
    let wanted = saturating_u16(label_width + gap + widest_value + BORDERS as usize);
    let width = wanted.clamp(floor, ceiling);
    let value_width = (width as usize)
        .saturating_sub(BORDERS as usize + label_width + gap)
        .max(8);

    // Where the selected field's lines start and end, so the view can be
    // scrolled to it once the wrapping is known.
    let mut selection = (0usize, 0usize);
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (index, column) in grid.result.columns.iter().enumerate() {
        let value = row.get(index);
        // The grid's cursor column is the selected field: the record opens on
        // whatever you were reading, and moving here moves the grid too.
        let focused = index == grid.column;
        if focused {
            selection.0 = lines.len();
        }
        let value_style = match value {
            Some(value) => results::value_style(value),
            None => theme::cell_text(),
        };
        let text = value.map(field_text).unwrap_or_default();

        for (offset, piece) in wrap(&text, value_width).into_iter().enumerate() {
            let label = if offset == 0 {
                pad(&ui::truncate(&column.name, label_width), label_width)
            } else {
                " ".repeat(label_width)
            };
            let mut spans = vec![
                Span::styled(
                    label,
                    if focused {
                        theme::title(true)
                    } else {
                        theme::muted()
                    },
                ),
                Span::raw(" ".repeat(gap)),
                Span::styled(piece, value_style),
            ];
            if focused {
                spans = spans
                    .into_iter()
                    .map(|span| {
                        let style = span.style.bg(theme::SURFACE);
                        Span::styled(span.content, style)
                    })
                    .collect();
            }
            lines.push(Line::from(spans));
        }
        if focused {
            selection.1 = lines.len().saturating_sub(1);
        }
    }

    // Height follows the wrapped line count, which is only known now: one row
    // per line, plus the footer and the two borders.
    let ceiling = (area.height * 88 / 100).max(BORDERS + 2);
    let height = saturating_u16(lines.len() + BORDERS as usize + 1)
        .clamp((BORDERS + 2).min(ceiling), ceiling);

    let fields = grid.result.columns.len();
    let inner = frame_for_counted(
        frame,
        ui::centered_size(area, width, height),
        &title,
        format!("{}/{fields} fields", grid.column + 1),
    );
    if inner.height < 2 {
        return;
    }

    let body_height = inner.height.saturating_sub(1) as usize;
    let max_scroll = lines.len().saturating_sub(body_height);

    let scroll = match app.overlay.as_mut() {
        Some(Overlay::Detail(detail)) => {
            // The end of the selected field first, then its start, so a field
            // taller than the box shows its label rather than its tail.
            let mut offset = ui::scroll_offset(detail.scroll, selection.1, body_height);
            offset = ui::scroll_offset(offset, selection.0, body_height);
            detail.scroll = offset.min(max_scroll);
            detail.scroll
        }
        _ => 0,
    };

    let visible: Vec<Line> = lines.into_iter().skip(scroll).take(body_height).collect();
    frame.render_widget(
        Paragraph::new(visible),
        Rect {
            height: inner.height - 1,
            ..inner
        },
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("↑↓", theme::key()),
            Span::styled(" field · ", theme::dim()),
            Span::styled("↵", theme::key()),
            Span::styled(" value · ", theme::dim()),
            Span::styled("←→", theme::key()),
            Span::styled(" record · ", theme::dim()),
            Span::styled("Esc", theme::key()),
            Span::styled(" close", theme::dim()),
        ])),
        Rect {
            y: inner.y + inner.height - 1,
            height: 1,
            ..inner
        },
    );
}

/// One field of the open record, in full.
///
/// The record modal gives every value the same narrow column and wraps it; a
/// JSON document or a long comment needs the whole box. JSON is re-indented
/// here, since a document stored on one line is not readable as one.
fn value(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(grid) = app.console().grid() else {
        app.overlay = None;
        return;
    };
    let (Some(row), Some(column)) = (
        grid.result.rows.get(grid.row),
        grid.result.columns.get(grid.column),
    ) else {
        return;
    };

    let cell = row.get(grid.column);
    let style = cell
        .map(results::value_style)
        .unwrap_or_else(theme::cell_text);
    let title = format!("{} {}", column.name, column.type_name);
    // The counter measures the stored value, not the re-indented one — the
    // size of a JSON document is not a property of how it is being displayed.
    let raw = cell.map(field_text).unwrap_or_default();
    let counter = format!("{} · row {}", value_size(&raw), grid.row + 1);
    let text = match cell {
        Some(cell) => expand(cell, raw),
        None => raw,
    };

    // Wide enough for the longest line the value has, so a re-indented
    // document is not re-wrapped on top of its own indentation.
    let widest = text
        .split('\n')
        .map(|line| UnicodeWidthStr::width(line.trim_end_matches('\r')))
        .max()
        .unwrap_or(0);
    let ceiling = (area.width * 88 / 100).max(1);
    let wanted = saturating_u16(widest + BORDERS as usize + PADDING * 2);
    let width = wanted.clamp(MIN_VALUE_WIDTH.min(ceiling), ceiling);
    let body_width = (width as usize).saturating_sub(BORDERS as usize + PADDING * 2);

    let lines: Vec<Line<'static>> = wrap(&text, body_width.max(8))
        .into_iter()
        .map(|piece| Line::from(Span::styled(piece, style)))
        .collect();

    let ceiling = (area.height * 88 / 100).max(BORDERS + 2);
    let height = saturating_u16(lines.len() + BORDERS as usize + 1)
        .clamp((BORDERS + 2).min(ceiling), ceiling);

    let inner = frame_for_counted(
        frame,
        ui::centered_size(area, width, height),
        &title,
        counter,
    );
    if inner.height < 2 {
        return;
    }

    let body_height = inner.height.saturating_sub(1) as usize;
    let max_scroll = lines.len().saturating_sub(body_height);
    let scroll = match app.overlay.as_mut() {
        Some(Overlay::Value(value)) => {
            value.max_scroll = max_scroll;
            value.scroll = value.scroll.min(max_scroll);
            value.scroll
        }
        _ => 0,
    };

    let visible: Vec<Line> = lines.into_iter().skip(scroll).take(body_height).collect();
    frame.render_widget(
        Paragraph::new(visible),
        Rect {
            x: inner.x + PADDING as u16,
            width: inner.width.saturating_sub(PADDING as u16 * 2),
            height: inner.height - 1,
            ..inner
        },
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("↑↓", theme::key()),
            Span::styled(" scroll · ", theme::dim()),
            Span::styled("←→", theme::key()),
            Span::styled(" record · ", theme::dim()),
            Span::styled("Esc", theme::key()),
            Span::styled(" back", theme::dim()),
        ])),
        Rect {
            x: inner.x + PADDING as u16,
            y: inner.y + inner.height - 1,
            width: inner.width.saturating_sub(PADDING as u16 * 2),
            height: 1,
        },
    );
}

/// Enough for the footer, so the box does not resize down under its own hints.
const MIN_VALUE_WIDTH: u16 = 40;

/// The value as the whole-value modal shows it: JSON re-indented, everything
/// else as it comes.
fn expand(value: &binsql_core::Value, text: String) -> String {
    match value {
        // A text column holds JSON as often as a JSON column does, so the
        // value decides this, not the type the driver reported.
        binsql_core::Value::Json(_) | binsql_core::Value::Text(_) => {
            pretty_json(&text).unwrap_or(text)
        }
        _ => text,
    }
}

fn pretty_json(text: &str) -> Option<String> {
    let trimmed = text.trim_start();
    if !trimmed.starts_with('{') && !trimmed.starts_with('[') {
        return None;
    }
    let parsed: serde_json::Value = serde_json::from_str(text).ok()?;
    serde_json::to_string_pretty(&parsed).ok()
}

/// How big the value is, for the counter in the modal's border.
fn value_size(text: &str) -> String {
    let chars = text.chars().count();
    let lines = text.split('\n').count();
    if lines > 1 {
        format!("{lines} lines · {chars} chars")
    } else {
        format!("{chars} chars")
    }
}

/// One field's value as text. NULL is spelled out here so it is styled and
/// measured like any other value.
fn field_text(value: &binsql_core::Value) -> String {
    match value {
        binsql_core::Value::Null => "NULL".to_string(),
        other => other.to_text(),
    }
}

/// A very long value would overflow a `u16`; it is going to be clamped to the
/// screen anyway.
fn saturating_u16(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

/// Wraps to `width` display columns, breaking between words where it can and
/// mid-token when a token is longer than the line. Embedded newlines in the
/// value are kept, since they are part of what makes a value worth opening.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    for paragraph in text.split('\n') {
        let paragraph = paragraph.trim_end_matches('\r');
        if paragraph.is_empty() {
            out.push(String::new());
            continue;
        }

        let mut line = String::new();
        let mut used = 0;
        for word in paragraph.split_inclusive(' ') {
            let word_width = UnicodeWidthStr::width(word);

            if used + word_width > width && used > 0 {
                out.push(std::mem::take(&mut line));
                used = 0;
            }
            // A single token wider than the line has to be split somewhere.
            if word_width > width {
                for ch in word.chars() {
                    let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
                    if used + ch_width > width && used > 0 {
                        out.push(std::mem::take(&mut line));
                        used = 0;
                    }
                    line.push(ch);
                    used += ch_width;
                }
                continue;
            }
            line.push_str(word);
            used += word_width;
        }
        out.push(line);
    }
    out
}

fn pad(text: &str, width: usize) -> String {
    let used = UnicodeWidthStr::width(text);
    format!("{}{}", text, " ".repeat(width.saturating_sub(used)))
}

/// The command palette, sized to what it is currently offering.
///
/// Pinned near the top rather than centred: the list shrinks as the query
/// narrows it, and a centred box would slide the prompt up the screen under
/// the cursor on every keystroke.
fn command_palette(frame: &mut Frame, palette: &Palette, area: Rect) {
    const FROM_TOP: u16 = 4;
    /// Enough of the list to choose from without the box owning the screen.
    const MAX_ROWS: usize = 14;

    let matches = palette.matches();
    let rows = matches.len().clamp(1, MAX_ROWS);

    // Prompt, a blank separator, then the list — the popup breathes rather
    // than starting hard against its own border.
    let mut lines = vec![
        Line::from(vec![
            Span::styled("› ", theme::accent()),
            Span::styled(palette.query.clone(), theme::cell_text()),
            Span::styled("▏", theme::accent()),
        ]),
        Line::from(""),
    ];

    let offset = ui::scroll_offset(0, palette.selected, rows);
    for (index, command) in matches.iter().enumerate().skip(offset).take(rows) {
        let selected = index == palette.selected;
        let hint = command.hint();

        let mut spans = vec![
            if selected {
                Span::styled(theme::SELECTION_BAR.to_string(), theme::selection_bar(true))
            } else {
                Span::raw(" ")
            },
            Span::styled(" ", theme::selection(selected)),
        ];

        let body = theme::cell_text();
        spans.push(Span::styled(
            command.label(),
            if selected {
                body.patch(theme::selection(true))
            } else {
                body
            },
        ));
        if !hint.is_empty() {
            spans.push(Span::styled(format!("  {hint}"), theme::dim()));
        }
        lines.push(Line::from(spans));
    }

    if matches.is_empty() {
        lines.push(Line::from(Span::styled("  No match", theme::dim())));
    }

    let width =
        saturating_u16(block_width(&lines) + BORDERS as usize + PADDING * 2).max(MIN_PALETTE_WIDTH);
    let height = saturating_u16(lines.len()) + BORDERS;

    let position = if matches.is_empty() {
        0
    } else {
        palette.selected + 1
    };
    let inner = frame_for_counted(
        frame,
        ui::anchored_size(area, width, height, FROM_TOP),
        "Commands",
        format!("{position}/{}", matches.len()),
    );
    if inner.height < 2 {
        return;
    }

    frame.render_widget(
        Paragraph::new(lines),
        Rect {
            x: inner.x + PADDING as u16,
            width: inner.width.saturating_sub(PADDING as u16 * 2),
            ..inner
        },
    );
}

/// Wide enough that the box does not jump about as the query narrows the list.
const MIN_PALETTE_WIDTH: u16 = 52;

fn connect(frame: &mut Frame, form: &ConnectForm, area: Rect) {
    let title = if form.editing.is_some() {
        "Edit data source"
    } else {
        "New data source"
    };

    let mut lines = Vec::new();
    for field in Field::ORDER {
        let active = field == form.field;
        let value = match field {
            Field::Folder => form.folder.clone(),
            Field::Name => form.name.clone(),
            Field::Dsn => form.dsn.clone(),
            Field::Backend => form.backend_display(),
            Field::ReadOnly => checkbox(form.read_only),
            Field::OpenOnStart => checkbox(form.open_on_start),
        };

        lines.push(Line::from(vec![
            Span::styled(if active { " ▸ " } else { "   " }, theme::accent()),
            Span::styled(format!("{:<20}", field.label()), theme::muted()),
            Span::styled(
                value,
                if active {
                    theme::selection(true)
                } else {
                    theme::cell_text()
                },
            ),
            Span::styled(if active { "▏" } else { "" }, theme::accent()),
        ]));
    }

    // One blank line before the footer, rather than one between every field:
    // the label column already separates them, and a form that double-spaces
    // six fields is mostly gaps.
    lines.push(Line::from(""));
    if let Some(error) = &form.error {
        lines.push(Line::from(Span::styled(error.clone(), theme::danger())));
        lines.push(Line::from(""));
    }

    lines.push(Line::from(vec![
        Span::styled("Tab", theme::key()),
        Span::styled(" field · ", theme::dim()),
        Span::styled("← →", theme::key()),
        Span::styled(" change · ", theme::dim()),
        Span::styled("Enter", theme::key()),
        Span::styled(" save · ", theme::dim()),
        Span::styled("Esc", theme::key()),
        Span::styled(" cancel", theme::dim()),
    ]));

    // A connection string is longer than any box worth opening, so the form
    // takes a sensible width and lets the value scroll inside its field rather
    // than stretching to hold it.
    let width = saturating_u16(block_width(&lines) + BORDERS as usize + PADDING * 2).clamp(
        MIN_FORM_WIDTH.min(area.width),
        (area.width * 80 / 100).max(1),
    );
    let height = saturating_u16(lines.len()) + BORDERS;
    let inner = frame_for(frame, ui::centered_size(area, width, height), title);

    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        Rect {
            x: inner.x + PADDING as u16,
            width: inner.width.saturating_sub(PADDING as u16 * 2),
            ..inner
        },
    );
}

/// Enough for the labels plus a connection string worth reading.
const MIN_FORM_WIDTH: u16 = 64;

fn checkbox(on: bool) -> String {
    if on {
        "[x] yes".into()
    } else {
        "[ ] no".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use binsql_core::Value;

    #[test]
    fn json_is_reindented_whichever_type_it_arrived_as() {
        let text = |value: &Value| expand(value, field_text(value));

        let json = Value::Json(r#"{"a":1}"#.into());
        assert_eq!(text(&json), "{\n  \"a\": 1\n}");
        // The same document in a text column reads the same way.
        let stored_as_text = Value::Text(r#"{"a":1}"#.into());
        assert_eq!(text(&stored_as_text), "{\n  \"a\": 1\n}");
    }

    #[test]
    fn text_that_is_not_json_is_left_alone() {
        let braces = Value::Text("{not json".into());
        assert_eq!(expand(&braces, field_text(&braces)), "{not json");

        let prose = Value::Text("a long comment".into());
        assert_eq!(expand(&prose, field_text(&prose)), "a long comment");
    }

    #[test]
    fn a_wrapped_word_keeps_every_character() {
        let wrapped = wrap("a supercalifragilistic word", 8);
        assert_eq!(
            wrapped.concat().replace(' ', ""),
            "asupercalifragilisticword"
        );
        assert!(
            wrapped
                .iter()
                .all(|line| UnicodeWidthStr::width(line.as_str()) <= 8)
        );
    }
}
