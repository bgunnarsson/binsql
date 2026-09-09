//! The header line.
//!
//! One place that answers "what am I connected to", so the pane titles do not
//! have to. Built from the same powerline segments as the status line, with the
//! product chip on the left and the connection tally on the right.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::theme;
use crate::ui;

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let mut left = vec![
        Span::styled(" BINSQL ", theme::chip(theme::EMPHASIS)),
        Span::styled(
            theme::PL_RIGHT.to_string(),
            theme::powerline(theme::EMPHASIS, theme::SURFACE),
        ),
    ];

    let console = app.console();
    match &console.source {
        Some(source) => {
            let session = app.sessions.get(source);
            left.push(Span::styled(
                format!(" {} {source}", theme::ICON_SERVER),
                theme::title(true).bg(theme::SURFACE),
            ));

            if let Some(catalog) = &console.catalog {
                left.push(Span::styled(
                    format!("  {} {catalog}", theme::ICON_DATABASE),
                    theme::muted().bg(theme::SURFACE),
                ));
            }

            if let Some(backend) = session.map(|session| session.backend()) {
                left.push(Span::styled(
                    format!("  {}", backend.label()),
                    theme::muted().bg(theme::SURFACE),
                ));
            }

            if app.config.get(source).is_some_and(|entry| entry.read_only) {
                left.push(Span::styled(
                    "  read-only",
                    theme::warning().bg(theme::SURFACE),
                ));
            }

            left.push(Span::styled(" ", theme::muted().bg(theme::SURFACE)));
        }
        None => {
            left.push(Span::styled(
                " no data source ",
                theme::muted().bg(theme::SURFACE),
            ));
        }
    }

    left.push(Span::styled(
        theme::PL_RIGHT.to_string(),
        theme::powerline(theme::SURFACE, theme::CHROME_BG),
    ));

    // Right: how many of the registered data sources are open.
    let connected = app.sessions.len();
    let total = app.config.connections.len();
    let tally = format!(" {connected}/{total} connected ");
    let right = vec![
        Span::styled(
            theme::PL_LEFT.to_string(),
            theme::powerline(theme::SURFACE, theme::CHROME_BG),
        ),
        Span::styled(tally.clone(), theme::muted().bg(theme::SURFACE)),
    ];

    let left_width = width_of(&left);
    let right_width = UnicodeWidthStr::width(tally.as_str()) + 1;
    let room = (area.width as usize).saturating_sub(right_width);

    let mut spans = if left_width > room {
        truncate_spans(left, room)
    } else {
        let mut spans = left;
        spans.push(Span::styled(
            " ".repeat(room - left_width),
            theme::status_bar(),
        ));
        spans
    };
    spans.extend(right);

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(theme::status_bar()),
        area,
    );
}

fn width_of(spans: &[Span<'_>]) -> usize {
    spans
        .iter()
        .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
        .sum()
}

/// Cuts a run of styled segments to fit, keeping the leading ones whole. The
/// product chip and the data source matter more than the trailing detail.
fn truncate_spans(spans: Vec<Span<'static>>, room: usize) -> Vec<Span<'static>> {
    let mut out = Vec::with_capacity(spans.len());
    let mut used = 0;

    for span in spans {
        let width = UnicodeWidthStr::width(span.content.as_ref());
        if used + width <= room {
            used += width;
            out.push(span);
            continue;
        }
        let remaining = room.saturating_sub(used);
        if remaining > 0 {
            let content = ui::truncate(span.content.as_ref(), remaining);
            out.push(Span::styled(content, span.style));
        }
        break;
    }
    out
}
