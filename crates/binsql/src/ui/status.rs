//! The status line.
//!
//! What just happened on the left, the keys on the right, in Claude Code's
//! register: quiet text, `·` between things, no chips and no arrows.
//!
//! It used to open with a coloured chip naming the focused pane — binvim's mode
//! block, borrowed. Pane focus is not a mode, and the focused pane already says
//! so with its border, so the chip was a second answer to a question nobody had
//! asked twice.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Tone};
use crate::theme;
use crate::ui;

/// Key, then what it does, so the key can carry the accent and the label a
/// foreground bright enough to read.
const HINTS: [(&str, &str); 5] = [
    ("⇥", "panes"),
    ("⌃R", "run"),
    ("⌃K", "commands"),
    ("F1", "help"),
    ("⌃Q", "quit"),
];

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    // The transient message, or the context it falls back to once the message
    // has aged out.
    let (message, message_style) = if app.status.is_stale() {
        (context(app), theme::muted())
    } else {
        (app.status.text.clone(), tone_style(app.status.tone))
    };

    let hints = hint_spans();
    let hints_width = width_of(&hints);
    let room = (area.width as usize).saturating_sub(hints_width + 1);

    let message = ui::truncate(&message, room.saturating_sub(1));
    let mut spans = vec![Span::styled(format!(" {message}"), message_style)];

    let used = width_of(&spans);
    if room > used {
        spans.push(Span::styled(" ".repeat(room - used), theme::status_bar()));
        spans.extend(hints);
        spans.push(Span::styled(" ", theme::status_bar()));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(theme::status_bar()),
        area,
    );
}

fn hint_spans() -> Vec<Span<'static>> {
    let mut spans = Vec::with_capacity(HINTS.len() * 3);
    for (index, (binding, what)) in HINTS.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(" · ", theme::dim()));
        }
        spans.push(Span::styled(*binding, theme::key()));
        spans.push(Span::styled(format!(" {what}"), theme::muted()));
    }
    spans
}

fn width_of(spans: &[Span<'_>]) -> usize {
    spans
        .iter()
        .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
        .sum()
}

/// What the bar shows once a transient message has aged out: where the cursor
/// is, rather than what just happened. The header carries the connection, so
/// this does not repeat it.
fn context(app: &App) -> String {
    match app.console().grid() {
        Some(grid) if grid.rows() > 0 => format!("row {} of {}", grid.row + 1, grid.rows()),
        _ => "⌃K for commands, F1 for help".to_string(),
    }
}

fn tone_style(tone: Tone) -> ratatui::style::Style {
    match tone {
        Tone::Info => theme::muted(),
        Tone::Success => theme::success(),
        Tone::Warning => theme::warning(),
        Tone::Error => theme::danger(),
    }
}
