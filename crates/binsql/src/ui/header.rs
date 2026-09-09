//! The header line.
//!
//! One place that answers "what am I connected to", so the pane titles do not
//! have to.
//!
//! Styled after Claude Code rather than binvim: the mark carries the only
//! colour, everything after it is quiet text separated by `·`, and there are no
//! chips or arrows. binvim's powerline segments exist to shout which *mode* is
//! active; binsql has no modes, so that machinery had nothing to say here.

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
        Span::styled(format!(" {} ", theme::MARK), theme::brand()),
        Span::styled("binsql", theme::muted()),
    ];

    let console = app.console();
    match &console.source {
        Some(source) => {
            left.push(separator());
            left.push(Span::styled(source.clone(), theme::title(true)));

            if let Some(catalog) = &console.catalog {
                left.push(separator());
                left.push(Span::styled(catalog.clone(), theme::muted()));
            }
            if let Some(backend) = app.sessions.get(source).map(|session| session.backend()) {
                left.push(separator());
                left.push(Span::styled(backend.label().to_string(), theme::muted()));
            }
            if app.config.get(source).is_some_and(|entry| entry.read_only) {
                left.push(separator());
                left.push(Span::styled("read-only", theme::warning()));
            }
        }
        None => {
            left.push(separator());
            left.push(Span::styled("no data source", theme::muted()));
        }
    }

    // Right: how many of the registered data sources are open.
    let connected = app.sessions.len();
    let total = app.config.connections.len();
    let tally = format!("{connected}/{total} connected ");

    let left_width = width_of(&left);
    let tally_width = UnicodeWidthStr::width(tally.as_str());
    let room = (area.width as usize).saturating_sub(tally_width);

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
    spans.push(Span::styled(tally, theme::muted()));

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(theme::status_bar()),
        area,
    );
}

fn separator() -> Span<'static> {
    Span::styled("  ·  ", theme::dim())
}

fn width_of(spans: &[Span<'_>]) -> usize {
    spans
        .iter()
        .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
        .sum()
}

/// Cuts a run of styled segments to fit, keeping the leading ones whole. The
/// mark and the data source matter more than the trailing detail.
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
