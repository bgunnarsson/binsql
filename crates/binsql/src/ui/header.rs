//! The header line.
//!
//! One question: where does ⌃R send this SQL. The left says what you call that
//! connection, the right says what it actually is — the engine that answers on
//! it and the host it lives on — because two connections nicknamed `prod` look
//! identical until something spells out which server each one reaches.
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
    let left = identity(app);
    let left_width = width_of(&left);

    // The detail gives way first, one item at a time: which connection this is
    // matters more than which build of the server answers on it.
    let mut details = details(app);
    let mut right = detail_spans(&details);
    while !details.is_empty() && left_width + width_of(&right) > area.width as usize {
        details.remove(0);
        right = detail_spans(&details);
    }

    let room = (area.width as usize).saturating_sub(width_of(&right));
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

/// What this connection is called, and whether it can be written to.
fn identity(app: &App) -> Vec<Span<'static>> {
    let mut spans = vec![Span::styled(format!(" {}  ", theme::MARK), theme::brand())];

    let console = app.console();
    let Some(source) = &console.source else {
        // Nothing is bound, so the header has nothing better to say than which
        // program this is.
        spans.push(Span::styled("binsql", theme::muted()));
        spans.push(separator());
        spans.push(Span::styled("no data source", theme::dim()));
        return spans;
    };

    spans.push(Span::styled(source.clone(), theme::title(true)));

    // A console that has not picked a catalog runs against whichever one the
    // connection opened on, so that is the one to name — the header would
    // otherwise go quiet about the database until a table happened to be
    // opened.
    let catalog = console.catalog.clone().or_else(|| {
        app.sessions
            .get(source)
            .and_then(|session| session.current_catalog().map(str::to_string))
    });
    if let Some(catalog) = &catalog {
        // A path, not a list: the catalog is inside the server, and `›` says so
        // where a `·` would make them look like two unrelated facts.
        spans.push(Span::styled(" › ", theme::dim()));
        spans.push(Span::styled(catalog.clone(), theme::muted()));
    }
    if app.config.get(source).is_some_and(|entry| entry.read_only) {
        spans.push(Span::raw("   "));
        spans.push(Span::styled(
            format!("{} read-only", theme::ICON_LOCK),
            theme::warning(),
        ));
    }
    spans
}

/// What the connection actually reaches: the engine, then the host. Ordered so
/// that dropping from the front leaves the more identifying half on screen.
fn details(app: &App) -> Vec<String> {
    let console = app.console();
    let Some(source) = &console.source else {
        return Vec::new();
    };

    let entry = app.config.get(source);
    let session = app.sessions.get(source);
    let mut out = Vec::new();

    // The backend is known from the config before anything connects; the
    // version only once a server has answered.
    if let Some(backend) = session.map(|s| s.backend()).or(entry.map(|e| e.backend)) {
        out.push(engine(backend, session.and_then(|s| s.server_version())));
    }
    if let Some(host) = entry.and_then(|e| binsql_core::dsn::display_host(e.backend, &e.dsn)) {
        out.push(host);
    }
    out
}

fn detail_spans(details: &[String]) -> Vec<Span<'static>> {
    if details.is_empty() {
        return Vec::new();
    }
    let mut spans = Vec::with_capacity(details.len() * 2);
    for (index, detail) in details.iter().enumerate() {
        if index > 0 {
            spans.push(separator());
        }
        spans.push(Span::styled(detail.clone(), theme::muted()));
    }
    spans.push(Span::raw(" "));
    spans
}

/// The engine, named the way anyone would say it out loud.
///
/// What a server reports about itself runs from a bare number to the first line
/// of a paragraph, so only the dotted number is taken from it — the label
/// already says which product it belongs to.
fn engine(backend: binsql_core::Backend, version: Option<&str>) -> String {
    let label = backend.label();
    match version.and_then(dotted_number) {
        Some(number) => format!("{label} {number}"),
        None => label.to_string(),
    }
}

/// The first run of digits and dots in a version string: `16.2` out of
/// `PostgreSQL 16.2`, `3.45.1` out of SQLite's bare number.
///
/// Taking the *first* run is what makes SQL Server read as `SQL Server 2022`
/// rather than `SQL Server 16.0.4115.5` — the release year comes before the
/// build in what it reports, and the year is what anybody calls it.
fn dotted_number(version: &str) -> Option<String> {
    let number: String = version
        .split(|ch: char| !ch.is_ascii_digit() && ch != '.')
        .find(|part| part.starts_with(|ch: char| ch.is_ascii_digit()))?
        .trim_end_matches('.')
        .to_string();
    Some(number).filter(|number| !number.is_empty())
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

#[cfg(test)]
mod tests {
    use super::*;
    use binsql_core::Backend;

    #[test]
    fn a_version_is_reduced_to_its_number() {
        assert_eq!(
            engine(Backend::Postgres, Some("PostgreSQL 16.2")),
            "PostgreSQL 16.2"
        );
        assert_eq!(engine(Backend::Sqlite, Some("3.45.1")), "SQLite 3.45.1");
        assert_eq!(
            engine(
                Backend::MsSql,
                Some("Microsoft SQL Server 2022 (RTM-CU12) - 16.0.4115.5 (X64)")
            ),
            "SQL Server 2022"
        );
    }

    #[test]
    fn an_engine_that_says_nothing_still_names_itself() {
        assert_eq!(engine(Backend::MySql, None), "MySQL");
        assert_eq!(engine(Backend::MySql, Some("unknown")), "MySQL");
    }
}
