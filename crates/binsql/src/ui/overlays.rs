use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};

use crate::app::App;
use crate::app::overlay::{ConnectForm, Field, Overlay, Palette};
use crate::theme;
use crate::ui;

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    match &app.overlay {
        None => {}
        Some(Overlay::Help) => help(frame, area),
        Some(Overlay::Detail) => detail(frame, app, area),
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
            ("Enter", "Show the full value"),
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

fn detail(frame: &mut Frame, app: &App, area: Rect) {
    let area = ui::centered(area, 70, 60);

    let (title, body, style) = match app.console().grid() {
        Some(grid) => {
            let column = grid
                .result
                .columns
                .get(grid.column)
                .map(|c| format!("{} · {}", c.name, c.type_name))
                .unwrap_or_else(|| "Value".to_string());
            match grid.selected_value() {
                Some(binsql_core::Value::Null) => (column, "NULL".to_string(), theme::cell_null()),
                Some(value) => (column, value.to_text(), theme::cell_text()),
                None => ("Value".into(), String::new(), theme::cell_text()),
            }
        }
        None => ("Value".into(), String::new(), theme::cell_text()),
    };

    let inner = frame_for(frame, area, &title);
    frame.render_widget(
        Paragraph::new(Span::styled(body, style)).wrap(Wrap { trim: false }),
        inner,
    );
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
