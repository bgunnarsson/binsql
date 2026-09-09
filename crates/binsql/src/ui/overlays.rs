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
        Some(Overlay::Help) => help(frame, area),
        // Takes `app` mutably: the renderer is what discovers how tall the
        // record is, and the scroll limit follows from that.
        Some(Overlay::Detail(_)) => detail(frame, app, area),
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

fn help(frame: &mut Frame, area: Rect) {
    let area = ui::centered(area, 88, 90);
    let inner = frame_for(frame, area, "Help");
    if inner.height < 3 {
        return;
    }

    // Two columns: the bindings do not fit down one, and a help screen that
    // scrolls is a help screen nobody reads to the end of.
    let columns = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Horizontal)
        .constraints([
            ratatui::layout::Constraint::Percentage(52),
            ratatui::layout::Constraint::Percentage(48),
        ])
        .split(Rect {
            height: inner.height - 1,
            ..inner
        });

    let split = 1; // "Anywhere" is the long one; it gets a column to itself.
    for (index, area) in columns.iter().enumerate() {
        let sections = if index == 0 {
            &SECTIONS[..split]
        } else {
            &SECTIONS[split..]
        };

        let mut lines = Vec::new();
        for (heading, bindings) in sections {
            lines.push(Line::from(Span::styled(
                (*heading).to_string(),
                theme::title(true),
            )));
            let room = (area.width as usize).saturating_sub(15);
            for (keys, description) in *bindings {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(format!("{keys:<13}"), theme::key()),
                    Span::styled(ui::truncate(description, room), theme::muted()),
                ]));
            }
            lines.push(Line::from(""));
        }
        frame.render_widget(Paragraph::new(lines), *area);
    }

    frame.render_widget(
        Paragraph::new(Span::styled("Any key closes this.", theme::dim())),
        Rect {
            y: inner.y + inner.height - 1,
            height: 1,
            ..inner
        },
    );
}

type Section = (&'static str, &'static [(&'static str, &'static str)]);

const SECTIONS: &[Section] = &[
    (
        "Anywhere",
        &[
            ("⌃R", "Run the query or selection"),
            ("⌃K", "Command palette"),
            ("⌃T / ⌃W", "New console / close console"),
            ("⌥1…9", "Jump to console"),
            ("⌃N", "New data source"),
            ("Tab / ⇧Tab", "Cycle panes"),
            ("⌥h ⌥k ⌥j", "Databases / query / results"),
            ("F1 or ?", "This help"),
            ("F5", "Refresh the selected node"),
            ("⌃Q", "Quit"),
        ],
    ),
    (
        "Databases",
        &[
            ("j / k", "Move"),
            ("l / Space", "Expand"),
            ("h", "Collapse, or go to parent"),
            ("Enter", "Connect, or open the table"),
            ("g / G", "First / last"),
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
            ("Esc / ⇧Tab", "Leave the editor"),
            ("Tab", "Indents, so it stays here"),
            ("⌃Z / ⌃Y", "Undo / redo"),
            ("Select text", "⌃R then runs only that"),
        ],
    ),
];

/// The selected row with every column stacked, label beside value.
///
/// The grid can only give a value one line and truncates to fit; this is where
/// a long comment or a wrapped connection string is actually readable.
fn detail(frame: &mut Frame, app: &mut App, area: Rect) {
    let area = ui::centered(area, 80, 80);

    let Some(grid) = app.console().grid() else {
        let inner = frame_for(frame, area, "Row");
        frame.render_widget(
            Paragraph::new(Span::styled("No rows.", theme::dim())),
            inner,
        );
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
    let inner = frame_for(frame, area, &title);
    if inner.height < 2 {
        return;
    }

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
    let value_width = (inner.width as usize)
        .saturating_sub(label_width + gap)
        .max(8);

    let Some(row) = grid.result.rows.get(grid.row) else {
        return;
    };

    let mut lines: Vec<Line<'static>> = Vec::new();
    for (index, column) in grid.result.columns.iter().enumerate() {
        let value = row.get(index);
        // The column the grid cursor was on stays marked, so opening the row
        // does not lose track of where you were.
        let focused = index == grid.column;
        let value_style = match value {
            Some(value) => results::value_style(value),
            None => theme::cell_text(),
        };
        let text = match value {
            Some(binsql_core::Value::Null) => "NULL".to_string(),
            Some(value) => value.to_text(),
            None => String::new(),
        };

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
                        let style = span.style.bg(theme::SURFACE0);
                        Span::styled(span.content, style)
                    })
                    .collect();
            }
            lines.push(Line::from(spans));
        }
    }

    let body_height = inner.height.saturating_sub(1) as usize;
    let max_scroll = lines.len().saturating_sub(body_height);

    let scroll = match app.overlay.as_mut() {
        Some(Overlay::Detail(detail)) => {
            detail.max_scroll = max_scroll;
            detail.scroll = detail.scroll.min(max_scroll);
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

    let more = if max_scroll > 0 {
        format!(" · {}/{}", scroll + 1, max_scroll + 1)
    } else {
        String::new()
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("↑↓", theme::key()),
            Span::styled(" fields · ", theme::dim()),
            Span::styled("←→", theme::key()),
            Span::styled(" record · ", theme::dim()),
            Span::styled("Esc", theme::key()),
            Span::styled(format!(" close{more}"), theme::dim()),
        ])),
        Rect {
            y: inner.y + inner.height - 1,
            height: 1,
            ..inner
        },
    );
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

fn command_palette(frame: &mut Frame, palette: &Palette, area: Rect) {
    let area = ui::centered(area, 60, 60);
    let inner = frame_for(frame, area, "Commands");
    if inner.height < 2 {
        return;
    }

    let mut lines = vec![
        Line::from(vec![
            Span::styled("› ", theme::accent()),
            Span::styled(palette.query.clone(), theme::cell_text()),
            Span::styled("▏", theme::accent()),
        ]),
        Line::from(""),
    ];

    let matches = palette.matches();
    let room = inner.height.saturating_sub(2) as usize;
    let offset = ui::scroll_offset(0, palette.selected, room);

    for (index, command) in matches.iter().enumerate().skip(offset).take(room) {
        let selected = index == palette.selected;
        let label = command.label();
        let hint = command.hint();

        let mut spans = vec![
            Span::styled(if selected { " ▸ " } else { "   " }, theme::accent()),
            Span::styled(
                ui::truncate(&label, inner.width.saturating_sub(12) as usize),
                if selected {
                    theme::selection(true)
                } else {
                    theme::cell_text()
                },
            ),
        ];
        if !hint.is_empty() {
            spans.push(Span::styled(format!("  {hint}"), theme::dim()));
        }
        lines.push(Line::from(spans));
    }

    if matches.is_empty() {
        lines.push(Line::from(Span::styled("  No match", theme::dim())));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn connect(frame: &mut Frame, form: &ConnectForm, area: Rect) {
    let area = ui::centered(area, 66, 60);
    let title = if form.editing.is_some() {
        "Edit data source"
    } else {
        "New data source"
    };
    let inner = frame_for(frame, area, title);

    let mut lines = Vec::new();
    for field in Field::ORDER {
        let active = field == form.field;
        let value = match field {
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
        lines.push(Line::from(""));
    }

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

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn checkbox(on: bool) -> String {
    if on {
        "[x] yes".into()
    } else {
        "[ ] no".into()
    }
}
