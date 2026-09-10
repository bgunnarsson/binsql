//! Mouse handling.
//!
//! The gesture worth having is dragging the sidebar's edge. A schema holding
//! `_NestedContentMigrationBackup` does not fit the width that suits one
//! holding `album` and `artist`, and which of the two you are looking at is not
//! something a layout rule can know — so it is left to the hand.
//!
//! The wheel scrolls whatever the pointer is over. That is here because turning
//! mouse reporting on takes the wheel away from the terminal, and a wheel that
//! did something before and nothing afterwards reads as a bug.
//!
//! Every handler says whether anything changed, so the loop can skip a redraw
//! for the events that mean nothing here.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

use super::App;

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
            app.dragging_divider = on_divider(app, event);
            false
        }
        MouseEventKind::Drag(MouseButton::Left) if app.dragging_divider => {
            // The border is drawn on the column under the pointer, so the
            // sidebar is one wider than the column itself.
            app.explorer_width = Some(event.column.saturating_add(1));
            true
        }
        MouseEventKind::Up(MouseButton::Left) => {
            app.dragging_divider = false;
            false
        }
        MouseEventKind::ScrollDown => scroll(app, event, WHEEL),
        MouseEventKind::ScrollUp => scroll(app, event, -WHEEL),
        _ => false,
    }
}

/// Whether a press landed on the seam between the sidebar and the workspace.
///
/// Two columns count: the sidebar's own right border and the pane border that
/// sits against it. One column is a hard thing to hit with a pointer, and the
/// two are drawn touching, so anyone aiming at the seam means either.
fn on_divider(app: &App, event: MouseEvent) -> bool {
    let explorer = app.panes.explorer;
    if explorer.width == 0 {
        return false;
    }
    let edge = explorer.right().saturating_sub(1);

    (event.column == edge || event.column == edge.saturating_add(1))
        && event.row >= explorer.y
        && event.row < explorer.bottom()
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

fn contains(area: ratatui::layout::Rect, (column, row): (u16, u16)) -> bool {
    column >= area.x && column < area.right() && row >= area.y && row < area.bottom()
}
