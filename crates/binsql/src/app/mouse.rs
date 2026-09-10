//! Mouse handling.
//!
//! The gestures worth having are the two pane seams. A schema holding
//! `_NestedContentMigrationBackup` does not fit the width that suits one
//! holding `album` and `artist`; a query you are still writing wants room the
//! same query's results do not. Neither is something a layout rule can know, so
//! both are left to the hand.
//!
//! The wheel scrolls whatever the pointer is over. That is here because turning
//! mouse reporting on takes the wheel away from the terminal, and a wheel that
//! did something before and nothing afterwards reads as a bug.
//!
//! Every handler says whether anything changed, so the loop can skip a redraw
//! for the events that mean nothing here.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use super::{App, Divider};

/// Rows the wheel moves per notch.
const WHEEL: isize = 3;

pub fn handle(app: &mut App, event: MouseEvent) -> bool {
    // A modal owns the screen while it is up. Resizing what is behind it is
    // not something anyone is asking for mid-dialogue.
    if app.overlay.is_some() {
        return false;
    }

    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            app.dragging = divider_at(app, event);
            false
        }
        MouseEventKind::Drag(MouseButton::Left) => match app.dragging {
            // The border is drawn on the column under the pointer, so the
            // sidebar is one wider than the column itself.
            Some(Divider::Sidebar) => {
                app.explorer_width = Some(event.column.saturating_add(1));
                true
            }
            // Likewise downwards, measured from the top of the query pane
            // rather than the top of the screen.
            Some(Divider::Results) => {
                let top = app.panes.editor.y;
                app.editor_height = Some(event.row.saturating_add(1).saturating_sub(top));
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

/// Which seam a press landed on, if either.
///
/// Two lines count for each: the pane's own border and the one drawn against
/// it. A single line is a hard thing to hit with a pointer, and the two are
/// touching, so anyone aiming at the seam means either.
fn divider_at(app: &App, event: MouseEvent) -> Option<Divider> {
    let explorer = app.panes.explorer;
    if explorer.width > 0 {
        let edge = explorer.right().saturating_sub(1);
        if (event.column == edge || event.column == edge.saturating_add(1))
            && event.row >= explorer.y
            && event.row < explorer.bottom()
        {
            return Some(Divider::Sidebar);
        }
    }

    let editor = app.panes.editor;
    if editor.height > 0 {
        let edge = editor.bottom().saturating_sub(1);
        if (event.row == edge || event.row == edge.saturating_add(1))
            && event.column >= editor.x
            && event.column < editor.right()
        {
            return Some(Divider::Results);
        }
    }

    None
}

/// Moves the selection in whichever pane the pointer is over, without taking
/// focus: reading ahead down the tree while the editor keeps the cursor is the
/// point of doing it with the wheel.
fn scroll(app: &mut App, event: MouseEvent, delta: isize) -> bool {
    let at = (event.column, event.row);

    if contains(app.panes.explorer, at) {
        app.tree.move_selection(delta);
        return true;
    }
    if contains(app.panes.results, at)
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
