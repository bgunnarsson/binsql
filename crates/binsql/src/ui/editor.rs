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
    let counter = Some(format!("{}/{}", app.active_console + 1, app.consoles.len()));
    let block = ui::body_pane(&title, counter, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let console = &mut app.consoles[app.active_console];
    // The cursor is only drawn in the pane that has focus, so two consoles
    // never look equally active.
    console.editor.set_cursor_style(if focused {
        theme::selection(true)
    } else {
        theme::body()
    });
    frame.render_widget(&console.editor, inner);
}

/// The header carries the connection, so the title only names the console —
/// and says when this one points somewhere other than the header does, which
/// is the case worth catching before pressing ⌃R.
fn binding_title(app: &App) -> String {
    match &app.console().source {
        Some(_) => "Query".to_string(),
        None => "Query · not connected".to_string(),
    }
}
