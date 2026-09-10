//! Mouse handling.
//!
//! Two seams to drag, and otherwise the ordinary things a pointer does: a
//! click takes focus and lands the cursor where it was aimed, a drag over the
//! query editor selects, and the wheel scrolls whatever it is over.
//!
//! Turning mouse reporting on takes all of that away from the terminal, so
//! none of it is decoration — a pointer that did something before and nothing
//! afterwards reads as a bug.
//!
//! Every handler says whether anything changed, so the loop can skip a redraw
//! for the events that mean nothing here.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use tui_textarea::CursorMove;

use super::{App, Drag, Pane};

/// Rows the wheel moves per notch.
const WHEEL: isize = 3;

pub fn handle(app: &mut App, event: MouseEvent) -> bool {
    // A modal owns the screen while it is up. Rearranging what is behind it is
    // not something anyone is asking for mid-dialogue.
    if app.overlay.is_some() {
        return false;
    }

    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => press(app, event),
        MouseEventKind::Drag(MouseButton::Left) => match app.dragging {
            // The border is drawn on the column under the pointer, so the
            // sidebar is one wider than the column itself.
            Some(Drag::Sidebar) => {
                app.explorer_width = Some(event.column.saturating_add(1));
                true
            }
            // Likewise downwards, measured from the top of the query pane
            // rather than the top of the screen.
            Some(Drag::Results) => {
                let top = app.panes.editor.y;
                app.editor_height = Some(event.row.saturating_add(1).saturating_sub(top));
                true
            }
            // The selection was started by the press; moving the cursor with
            // one open is what extends it.
            Some(Drag::Text) => {
                let at = text_position(app, event);
                app.console_mut()
                    .editor
                    .move_cursor(CursorMove::Jump(at.0, at.1));
                true
            }
            None => false,
        },
        MouseEventKind::Up(MouseButton::Left) => {
            app.dragging = None;
            false
        }
        MouseEventKind::ScrollDown => scroll(app, event, WHEEL),
        MouseEventKind::ScrollUp => scroll(app, event, -WHEEL),
        _ => false,
    }
}

/// A press takes focus and puts the cursor where it was aimed.
fn press(app: &mut App, event: MouseEvent) -> bool {
    if let Some(seam) = divider_at(app, event) {
        app.dragging = Some(seam);
        return false;
    }

    let at = (event.column, event.row);

    if contains(app.panes.tree, at) {
        app.focus = Pane::Explorer;
        let row = (event.row - app.panes.tree.y) as usize;
        app.tree.select_visible(app.tree.offset + row);
        return true;
    }

    if contains(app.panes.text, at) {
        app.focus = Pane::Editor;
        let (row, column) = text_position(app, event);
        let editor = &mut app.console_mut().editor;
        // Cancel first: moving the cursor with a selection open would extend
        // the old one rather than start a new one here.
        editor.cancel_selection();
        editor.move_cursor(CursorMove::Jump(row, column));
        editor.start_selection();
        app.dragging = Some(Drag::Text);
        return true;
    }

    if contains(app.panes.grid, at) {
        app.focus = Pane::Results;
        select_cell(app, event);
        return true;
    }

    false
}

/// Where in the text a pointer at this column and row is.
///
/// Clamped by `CursorMove::Jump`, which lands on the nearest real position, so
/// a click past the end of a short line goes to the end of it.
fn text_position(app: &App, event: MouseEvent) -> (u16, u16) {
    let text = app.panes.text;
    let (top, left) = app.console().editor_scroll;

    let row = event.row.saturating_sub(text.y) as usize + top;
    let column = event.column.saturating_sub(text.x) as usize + left;
    (
        row.min(u16::MAX as usize) as u16,
        column.min(u16::MAX as usize) as u16,
    )
}

/// Moves the grid cursor to the cell under the pointer. The header row is not
/// a cell, so a click on it only takes focus.
fn select_cell(app: &mut App, event: MouseEvent) {
    let area = app.panes.grid;
    let Some(grid) = app.console_mut().grid_mut() else {
        return;
    };
    if event.row <= area.y {
        return;
    }

    let row = grid.row_offset + (event.row - area.y - 1) as usize;
    if row < grid.rows() {
        grid.row = row;
    }
    if let Some(column) = grid.column_at(event.column, area) {
        grid.column = column;
    }
}

/// Which seam a press landed on, if either.
///
/// Two lines count for each: the pane's own border and the one drawn against
/// it. A single line is a hard thing to hit with a pointer, and the two are
/// touching, so anyone aiming at the seam means either.
fn divider_at(app: &App, event: MouseEvent) -> Option<Drag> {
    let explorer = app.panes.explorer;
    if explorer.width > 0 {
        let edge = explorer.right().saturating_sub(1);
        if (event.column == edge || event.column == edge.saturating_add(1))
            && event.row >= explorer.y
            && event.row < explorer.bottom()
        {
            return Some(Drag::Sidebar);
        }
    }

    let editor = app.panes.editor;
    if editor.height > 0 {
        let edge = editor.bottom().saturating_sub(1);
        if (event.row == edge || event.row == edge.saturating_add(1))
            && event.column >= editor.x
            && event.column < editor.right()
        {
            return Some(Drag::Results);
        }
    }

    None
}

/// Scrolls whatever the pointer is over, without taking focus: reading ahead
/// down the tree while the editor keeps the cursor is the point of doing it
/// with the wheel.
fn scroll(app: &mut App, event: MouseEvent, delta: isize) -> bool {
    let at = (event.column, event.row);

    if contains(app.panes.tree, at) {
        app.tree.move_selection(delta);
        return true;
    }
    if contains(app.panes.text, at) {
        let rows = delta.clamp(i16::MIN as isize, i16::MAX as isize) as i16;
        let console = app.console_mut();
        console
            .editor
            .scroll(tui_textarea::Scrolling::Delta { rows, cols: 0 });
        // The widget moved its own viewport by exactly this, and the copy has
        // to take the same step: re-deriving from the cursor alone would not
        // catch it, because scrolling drags the cursor along and leaves it
        // inside the window either way.
        console.editor_scroll.0 = console.editor_scroll.0.saturating_add_signed(delta);
        return true;
    }
    if contains(app.panes.grid, at)
        && let Some(grid) = app.console_mut().grid_mut()
    {
        grid.move_row(delta);
        return true;
    }
    false
}

fn contains(area: Rect, (column, row): (u16, u16)) -> bool {
    column >= area.x && column < area.right() && row >= area.y && row < area.bottom()
}
