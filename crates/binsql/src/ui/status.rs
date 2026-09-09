//! The status line, in binvim's powerline form.
//!
//! A bright chip naming the focused pane, then segments stepping down through
//! surface and chrome, each separated by a filled arrow drawn in the outgoing
//! segment's colour over the incoming one's. The right end carries the key
//! hints, entered with a left-pointing arrow.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Pane, Tone};
use crate::theme;
use crate::ui;

/// Key, then what it does. Split so the key can carry the accent and the
/// label a foreground bright enough to read — `dim` on `surface` is very
/// nearly the same colour.
const HINTS: [(&str, &str); 5] = [
    ("⇥", "panes"),
    ("⌃R", "run"),
    ("⌃K", "commands"),
    ("F1", "help"),
    ("⌃Q", "quit"),
];

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let pane_colour = pane_colour(app.focus);
    let message_colour = theme::SURFACE;

    let mut spans = vec![
        Span::styled(
            format!(" {} ", pane_name(app.focus)),
            theme::chip(pane_colour),
        ),
        Span::styled(
            theme::PL_RIGHT.to_string(),
            theme::powerline(pane_colour, message_colour),
        ),
    ];

    // The transient message, or the context it falls back to once the message
    // has aged out.
    let (message, message_style) = if app.status.is_stale() {
        (context(app), theme::muted())
    } else {
        (app.status.text.clone(), tone_style(app.status.tone))
    };
    let message = ui::truncate(&message, area.width.saturating_sub(24) as usize);
    spans.push(Span::styled(
        format!(" {message} "),
        message_style.bg(message_colour),
    ));
    spans.push(Span::styled(
        theme::PL_RIGHT.to_string(),
        theme::powerline(message_colour, theme::CHROME_BG),
    ));

    let hints = hint_spans();
    let used: usize = spans
        .iter()
        .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
        .sum();
    let hints_width: usize = hints
        .iter()
        .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
        .sum::<usize>()
        + 1;
    let room = (area.width as usize).saturating_sub(used);

    if room > hints_width {
        spans.push(Span::styled(
            " ".repeat(room - hints_width),
            theme::status_bar(),
        ));
        spans.push(Span::styled(
            theme::PL_LEFT.to_string(),
            theme::powerline(theme::SURFACE, theme::CHROME_BG),
        ));
        spans.extend(hints);
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(theme::status_bar()),
        area,
    );
}

/// The hint strip: accent keys against readable labels, both on `surface`.
fn hint_spans() -> Vec<Span<'static>> {
    let label = theme::muted().bg(theme::SURFACE);
    let key = theme::key().bg(theme::SURFACE);

    let mut spans = vec![Span::styled(" ", label)];
    for (index, (binding, what)) in HINTS.iter().enumerate() {
        if index > 0 {
            // The separator takes the label's colour rather than `dim`: on
            // `surface`, `dim` is very nearly the background.
            spans.push(Span::styled(" · ", label));
        }
        spans.push(Span::styled(*binding, key));
        spans.push(Span::styled(format!(" {what}"), label));
    }
    spans.push(Span::styled(" ", label));
    spans
}

fn pane_name(pane: Pane) -> &'static str {
    match pane {
        Pane::Explorer => "DATABASES",
        Pane::Editor => "QUERY",
        Pane::Results => "RESULTS",
    }
}

/// One colour per pane, the way binvim gives one per mode — so the chip says
/// where you are before you have read the word.
fn pane_colour(pane: Pane) -> Color {
    match pane {
        Pane::Explorer => theme::HINT,
        Pane::Editor => theme::ACCENT_SECONDARY,
        Pane::Results => theme::EMPHASIS,
    }
}

/// What the bar shows once a transient message has aged out: where you are,
/// rather than what just happened.
fn context(app: &App) -> String {
    let console = app.console();
    let mut parts = Vec::new();
    if let Some(source) = &console.source {
        parts.push(source.clone());
    }
    if let Some(catalog) = &console.catalog {
        parts.push(catalog.clone());
    }
    if parts.is_empty() {
        return "No data source — ⌃N to add one".to_string();
    }
    parts.join(" · ")
}

fn tone_style(tone: Tone) -> ratatui::style::Style {
    match tone {
        Tone::Info => theme::muted(),
        Tone::Success => theme::success(),
        Tone::Warning => theme::warning(),
        Tone::Error => theme::danger(),
    }
}
