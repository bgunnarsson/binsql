use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{App, Pane};
use crate::theme;
use crate::ui;

pub fn draw_tabs(frame: &mut Frame, app: &App, area: Rect) {
    let mut spans = Vec::new();
    for (index, console) in app.consoles.iter().enumerate() {
        let active = index == app.active_console;
        let marker = if console.is_running() { "◐ " } else { "" };
        spans.push(Span::styled(
            format!(" {marker}{} ", ui::truncate(&console.title, 24)),
            theme::tab(active),
        ));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(" + ", theme::dim()));

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(theme::status_bar()),
        area,
    );
}

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Pane::Editor;
    let title = binding_title(app);
    let block = ui::pane(&title, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let console = &mut app.consoles[app.active_console];
    // The cursor is only drawn in the pane that has focus, so two consoles
    // never look equally active.
    console.editor.set_cursor_style(if focused {
        theme::selection(true)
    } else {
        theme::panel()
    });
    frame.render_widget(&console.editor, inner);
}

/// The editor's title says what the console is pointed at, because that is the
/// one thing you must know before pressing ⌃R.
fn binding_title(app: &App) -> String {
    let console = app.console();
    match (&console.source, &console.catalog) {
        (Some(source), Some(catalog)) => {
            let read_only = app.config.get(source).is_some_and(|entry| entry.read_only);
            let suffix = if read_only { " · read-only" } else { "" };
            format!("Query · {source} / {catalog}{suffix}")
        }
        (Some(source), None) => format!("Query · {source}"),
        _ => "Query · not connected".to_string(),
    }
}
