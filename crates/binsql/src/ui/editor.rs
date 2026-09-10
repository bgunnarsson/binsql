use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{App, Pane, TabSpan};
use crate::theme;
use crate::ui;
use unicode_width::UnicodeWidthStr;

/// Space between one tab and the next.
const TAB_GAP: u16 = 1;
/// The button that opens a console, and what it is drawn as.
const NEW_TAB: &str = " + ";

pub fn draw_tabs(frame: &mut Frame, app: &mut App, area: Rect) {
    let mut spans = Vec::new();
    // Measured while they are laid out: a tab is as wide as its own title, so
    // where one ends is not something a click can work out for itself.
    let mut tabs = Vec::with_capacity(app.consoles.len() + 1);
    let mut x = area.x;

    for (index, console) in app.consoles.iter().enumerate() {
        let active = index == app.active_console;
        let marker = if console.is_running() { "◐ " } else { "" };
        let label = format!(" {marker}{} ", ui::truncate(&console.title, 24));
        let width = UnicodeWidthStr::width(label.as_str()) as u16;

        tabs.push(TabSpan {
            console: Some(index),
            start: x,
            end: x + width,
        });
        x += width + TAB_GAP;

        spans.push(Span::styled(label, theme::tab(active)));
        spans.push(Span::raw(" "));
    }

    tabs.push(TabSpan {
        console: None,
        start: x,
        end: x + UnicodeWidthStr::width(NEW_TAB) as u16,
    });
    spans.push(Span::styled(NEW_TAB, theme::dim()));

    app.tabs = tabs;
    app.panes.tabs = area;

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

    app.panes.text = inner;

    let console = &mut app.consoles[app.active_console];
    // The cursor is only drawn in the pane that has focus, so two consoles
    // never look equally active.
    console.editor.set_cursor_style(if focused {
        theme::selection(true)
    } else {
        theme::body()
    });

    // Keep our copy of the viewport level with the widget's, which it does not
    // expose. Same rule, same inputs, re-derived here every frame — see
    // `Console::editor_scroll`.
    let (row, column) = console.editor.cursor();
    console.editor_scroll = (
        ui::scroll_offset(console.editor_scroll.0, row, inner.height as usize),
        ui::scroll_offset(console.editor_scroll.1, column, inner.width as usize),
    );

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
