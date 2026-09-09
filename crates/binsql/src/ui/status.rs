use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{App, Pane, Tone};
use crate::theme;
use crate::ui;

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let mut left = vec![Span::styled(
        format!(" {} ", pane_name(app.focus)),
        theme::header(),
    )];

    left.push(Span::raw(" "));
    if app.status.is_stale() {
        left.push(Span::styled(context(app), theme::muted()));
    } else {
        left.push(Span::styled(
            app.status.text.clone(),
            tone_style(app.status.tone),
        ));
    }

    // "⌃T tab" used to sit here and read as the Tab key rather than a new
    // console, while the one binding people reach for first — moving between
    // panes — was not mentioned at all.
    let hints = " ⇥ panes · ⌃R run · ⌃K commands · ⌃T console · F1 help · ⌃Q quit ";
    let used: usize = left.iter().map(|span| span.content.chars().count()).sum();
    let room = (area.width as usize).saturating_sub(used);

    let mut spans = left;
    if room > hints.chars().count() {
        spans.push(Span::raw(" ".repeat(room - hints.chars().count())));
        spans.push(Span::styled(hints, theme::dim()));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(theme::status_bar()),
        area,
    );
}

fn pane_name(pane: Pane) -> &'static str {
    match pane {
        Pane::Explorer => "DATABASES",
        Pane::Editor => "QUERY",
        Pane::Results => "RESULTS",
    }
}

/// What the bar falls back to once a transient message has aged out: where you
/// are, rather than what just happened.
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
    ui::truncate(&parts.join(" · "), 60)
}

fn tone_style(tone: Tone) -> ratatui::style::Style {
    match tone {
        Tone::Info => theme::muted(),
        Tone::Success => theme::success(),
        Tone::Warning => theme::warning(),
        Tone::Error => theme::danger(),
    }
}
