//! Key handling.
//!
//! Three panes, each with its own bindings, plus a set that works anywhere.
//! Modifiers are chosen so nothing shadows ordinary typing in the editor: pane
//! movement is on Alt, not Ctrl+hjkl, because ⌃K belongs to the palette.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use tui_textarea::{Input, Key};

use super::overlay::{Command, ConnectForm, Field, Overlay, Palette, RowDetail, ValueDetail};
use super::{App, Pane};

/// How far ⌃D and ⌃U move.
const HALF_PAGE: isize = 10;

pub fn handle(app: &mut App, key: KeyEvent) {
    if key.kind != KeyEventKind::Press {
        return;
    }

    // Quit is handled before anything else can claim it. Raw mode has already
    // taken ⌃C away from the terminal — it arrives here as a key like any
    // other, and cancels a query rather than the program — so a modal that
    // swallowed ⌃Q would leave no way out of the program at all.
    if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.should_quit = true;
        return;
    }

    if app.overlay.is_some() {
        overlay(app, key);
        return;
    }
    if global(app, key) {
        return;
    }
    match app.focus {
        Pane::Explorer => explorer(app, key),
        Pane::Editor => editor(app, key),
        Pane::Results => results(app, key),
    }
}

/// Returns true when the key was consumed.
fn global(app: &mut App, key: KeyEvent) -> bool {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    match (key.code, ctrl, alt) {
        (KeyCode::Char('k'), true, _) => {
            app.overlay = Some(Overlay::Palette(Palette::build(app)));
        }
        (KeyCode::Char('r'), true, _) => app.run_query(),
        (KeyCode::Char('c'), true, _) => app.cancel_query(),
        (KeyCode::Char('t'), true, _) => app.new_console(),
        (KeyCode::Char('w'), true, _) => app.close_console(),
        (KeyCode::Char('n'), true, _) => {
            app.overlay = Some(Overlay::Connect(ConnectForm::new(&app.config)));
        }
        (KeyCode::F(1), _, _) => app.overlay = Some(Overlay::Help),
        (KeyCode::F(5), _, _) => app.refresh_selected(),

        (KeyCode::Char('h'), _, true) => app.focus = Pane::Explorer,
        (KeyCode::Char('l'), _, true) => app.focus = Pane::Editor,
        (KeyCode::Char('k'), _, true) => app.focus = Pane::Editor,
        (KeyCode::Char('j'), _, true) => app.focus = Pane::Results,

        (KeyCode::Char(digit @ '1'..='9'), _, true) => {
            app.select_console(digit as usize - '1' as usize);
        }
        (KeyCode::PageUp, true, _) => app.cycle_console(-1),
        (KeyCode::PageDown, true, _) => app.cycle_console(1),

        (KeyCode::BackTab, _, _) => cycle_focus(app, -1),
        // Tab indents in the editor, so it only cycles panes elsewhere.
        (KeyCode::Tab, _, _) if app.focus != Pane::Editor => cycle_focus(app, 1),

        _ => return false,
    }
    true
}

fn cycle_focus(app: &mut App, delta: isize) {
    let order = [Pane::Explorer, Pane::Editor, Pane::Results];
    let position = order.iter().position(|p| *p == app.focus).unwrap_or(0) as isize;
    let next = (position + delta).rem_euclid(order.len() as isize);
    app.focus = order[next as usize];
}

fn explorer(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.tree.move_selection(1),
        KeyCode::Char('k') | KeyCode::Up => app.tree.move_selection(-1),
        KeyCode::Char('d') if ctrl => app.tree.move_selection(HALF_PAGE),
        KeyCode::Char('u') if ctrl => app.tree.move_selection(-HALF_PAGE),
        KeyCode::PageDown => app.tree.move_selection(HALF_PAGE * 2),
        KeyCode::PageUp => app.tree.move_selection(-HALF_PAGE * 2),
        KeyCode::Char('g') | KeyCode::Home => app.tree.select_first(),
        KeyCode::Char('G') | KeyCode::End => app.tree.select_last(),

        KeyCode::Char('h') | KeyCode::Left => app.tree.collapse_or_parent(),
        KeyCode::Char('l') | KeyCode::Right | KeyCode::Char(' ') => app.toggle_selected(),
        KeyCode::Enter => app.activate_selected(),

        KeyCode::Char('r') => app.refresh_selected(),
        KeyCode::Char('n') => app.overlay = Some(Overlay::Connect(ConnectForm::new(&app.config))),
        KeyCode::Char('e') => edit_selected_source(app),
        KeyCode::Char('d') => disconnect_selected_source(app),
        KeyCode::Char('?') => app.overlay = Some(Overlay::Help),
        _ => {}
    }
}

fn edit_selected_source(app: &mut App) {
    let Some(name) = app.context_source() else {
        return;
    };
    let Some(source) = app.config.get(&name).cloned() else {
        return;
    };
    app.overlay = Some(Overlay::Connect(ConnectForm::editing(
        &name,
        &source,
        &app.config,
    )));
}

fn disconnect_selected_source(app: &mut App) {
    let Some(name) = app.context_source() else {
        return;
    };
    if app.sessions.contains_key(&name) {
        app.disconnect(&name);
    }
}

/// The editor's own bindings, then everything else through to the textarea.
///
/// All on control keys. ⌘ was tried and does not survive the trip: it needs the
/// kitty keyboard protocol to be encodable at all, and a macOS terminal claims
/// most of it before an application ever sees it.
fn editor(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let cmd = key.modifiers.contains(KeyModifiers::SUPER);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);

    match key.code {
        KeyCode::Esc => {
            app.focus = Pane::Explorer;
            return;
        }
        // Undo and redo are bound here rather than left to the textarea, whose
        // own map is emacs': undo on ⌃U and redo on ⌃R. ⌃R is binsql's "run
        // the query" and never reaches it, ⌃U is delete-to-line-start above,
        // and ⌃Z — which this program has always told people to press — was
        // bound to nothing at all.
        //
        // Redo comes first, or the shifted chord matches the plainer arm. The
        // shifted letter arrives in either case depending on the terminal.
        KeyCode::Char('z' | 'Z') if ctrl && shift => {
            app.console_mut().redo();
            return;
        }
        KeyCode::Char('z') if ctrl => {
            app.console_mut().undo();
            return;
        }
        // ⌃⇧Z and ⌃Z are the same byte to a terminal speaking the old
        // encoding, so the shifted spelling only works where the kitty
        // protocol does. ⌃Y is the redo that works everywhere. It costs the
        // textarea's yank-buffer recall, which is not the system clipboard and
        // is no loss; a real paste arrives bracketed and still works.
        KeyCode::Char('y') if ctrl => {
            app.console_mut().redo();
            return;
        }
        // The textarea puts "move to start of line" here, which Home already
        // does. Select-all is what a modern editor means by it, and typing
        // over a selection replaces it.
        KeyCode::Char('a') if ctrl => {
            app.console_mut().editor.select_all();
            return;
        }
        // Readline's spelling of delete-to-line-start, and ⌫ with a modifier,
        // which is what a hand reaches for.
        KeyCode::Char('u') | KeyCode::Backspace if ctrl => {
            app.console_mut().editor.delete_line_by_head();
            return;
        }
        _ => {}
    }

    // A ⌘ chord the editor has no use for is still not text. The textarea has
    // no concept of the modifier, so passing one through drops it and types
    // the bare letter into the query — ⌘S would write an "s".
    if cmd {
        return;
    }

    let input = Input::from(key);
    // The textarea maps Enter to a newline; nothing here should reach it that
    // the global bindings already claimed.
    if input.key == Key::Null {
        return;
    }
    app.console_mut().editor.input(input);
}

fn results(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let Some(grid) = app.console_mut().grid_mut() else {
        if key.code == KeyCode::Char('?') {
            app.overlay = Some(Overlay::Help);
        }
        return;
    };

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => grid.move_row(1),
        KeyCode::Char('k') | KeyCode::Up => grid.move_row(-1),
        KeyCode::Char('h') | KeyCode::Left => grid.move_column(-1),
        KeyCode::Char('l') | KeyCode::Right => grid.move_column(1),
        KeyCode::Char('d') if ctrl => grid.move_row(HALF_PAGE),
        KeyCode::Char('u') if ctrl => grid.move_row(-HALF_PAGE),
        KeyCode::PageDown => grid.move_row(HALF_PAGE * 2),
        KeyCode::PageUp => grid.move_row(-HALF_PAGE * 2),
        KeyCode::Char('g') | KeyCode::Home => grid.first_row(),
        KeyCode::Char('G') | KeyCode::End => grid.last_row(),
        KeyCode::Char('0') => grid.first_column(),
        KeyCode::Char('$') => grid.last_column(),
        KeyCode::Enter => app.overlay = Some(Overlay::Detail(RowDetail::default())),
        KeyCode::Char('?') => app.overlay = Some(Overlay::Help),
        _ => {}
    }
}

fn overlay(app: &mut App, key: KeyEvent) {
    match app.overlay.as_mut() {
        // The help screen says "any key closes this" and it has to be true,
        // or the way out is a guess. The splash is the same: it is a greeting,
        // not a prompt.
        Some(Overlay::Help) | Some(Overlay::Splash) => app.overlay = None,

        // The record is a list of its fields, so up and down move between them
        // rather than scrolling the box freely — the view follows the
        // selection, and Enter opens whichever field is under it.
        Some(Overlay::Detail(_)) => match key.code {
            KeyCode::Char('j') | KeyCode::Down => move_field(app, 1),
            KeyCode::Char('k') | KeyCode::Up => move_field(app, -1),
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                move_field(app, HALF_PAGE)
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                move_field(app, -HALF_PAGE)
            }
            KeyCode::PageDown => move_field(app, HALF_PAGE * 2),
            KeyCode::PageUp => move_field(app, -HALF_PAGE * 2),
            KeyCode::Char('g') | KeyCode::Home => field_edge(app, false),
            KeyCode::Char('G') | KeyCode::End => field_edge(app, true),

            // Left and right step between records without closing, which is
            // the point of a row viewer when you are reading down a table.
            KeyCode::Char('h') | KeyCode::Left => step_record(app, -1),
            KeyCode::Char('l') | KeyCode::Right => step_record(app, 1),

            KeyCode::Enter => open_value(app),
            KeyCode::Esc => app.overlay = None,
            _ => {}
        },

        Some(Overlay::Value(value)) => match key.code {
            KeyCode::Char('j') | KeyCode::Down => value.scroll_by(1),
            KeyCode::Char('k') | KeyCode::Up => value.scroll_by(-1),
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                value.scroll_by(HALF_PAGE)
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                value.scroll_by(-HALF_PAGE)
            }
            KeyCode::PageDown => value.scroll_by(HALF_PAGE * 2),
            KeyCode::PageUp => value.scroll_by(-HALF_PAGE * 2),
            KeyCode::Char('g') | KeyCode::Home => value.to_top(),
            KeyCode::Char('G') | KeyCode::End => value.to_bottom(),

            // The same field of the next record, which is how one column gets
            // read down a table without closing and reopening at every row.
            KeyCode::Char('h') | KeyCode::Left => step_record(app, -1),
            KeyCode::Char('l') | KeyCode::Right => step_record(app, 1),

            KeyCode::Esc | KeyCode::Enter => close_value(app),
            _ => {}
        },
        Some(Overlay::Palette(palette)) => match key.code {
            KeyCode::Esc => app.overlay = None,
            KeyCode::Up => palette.move_selection(-1),
            KeyCode::Down => palette.move_selection(1),
            KeyCode::Backspace => palette.backspace(),
            KeyCode::Enter => {
                let command = palette.selected_command();
                app.overlay = None;
                if let Some(command) = command {
                    execute(app, command);
                }
            }
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                palette.push(ch);
            }
            _ => {}
        },
        Some(Overlay::Connect(form)) => match key.code {
            KeyCode::Esc => app.overlay = None,
            KeyCode::Tab | KeyCode::Down => form.next_field(1),
            KeyCode::BackTab | KeyCode::Up => form.next_field(-1),
            KeyCode::Left if form.field == Field::Backend => form.cycle_backend(-1),
            KeyCode::Right if form.field == Field::Backend => form.cycle_backend(1),
            KeyCode::Left | KeyCode::Right => form.toggle(),
            KeyCode::Backspace => form.backspace(),
            KeyCode::Enter => save_form(app),
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => form.push(ch),
            _ => {}
        },
        None => {}
    }
}

/// Moves the grid cursor to the next or previous row and shows that record.
/// The grid keeps the new position after the modal closes, so paging through
/// records here does not lose your place.
fn step_record(app: &mut App, delta: isize) {
    let Some(grid) = app.console_mut().grid_mut() else {
        return;
    };
    let before = grid.row;
    grid.move_row(delta);
    let moved = grid.row != before;
    if !moved {
        return;
    }

    match app.overlay.as_mut() {
        Some(Overlay::Detail(detail)) => detail.scroll = 0,
        // A different record's value is a different document; carrying the old
        // scroll into it lands somewhere arbitrary.
        Some(Overlay::Value(value)) => value.to_top(),
        _ => {}
    }
}

/// Moves the selected field. That is the grid's cursor column: the record
/// modal marks it, and the value modal opens it.
fn move_field(app: &mut App, delta: isize) {
    if let Some(grid) = app.console_mut().grid_mut() {
        grid.move_column(delta);
    }
}

fn field_edge(app: &mut App, last: bool) {
    let Some(grid) = app.console_mut().grid_mut() else {
        return;
    };
    if last {
        grid.last_column();
    } else {
        grid.first_column();
    }
}

fn open_value(app: &mut App) {
    let row = match app.overlay.take() {
        Some(Overlay::Detail(row)) => row,
        other => {
            app.overlay = other;
            return;
        }
    };
    app.overlay = Some(Overlay::Value(ValueDetail::new(row)));
}

fn close_value(app: &mut App) {
    let value = match app.overlay.take() {
        Some(Overlay::Value(value)) => value,
        other => {
            app.overlay = other;
            return;
        }
    };
    app.overlay = Some(Overlay::Detail(value.row));
}

fn save_form(app: &mut App) {
    let Some(Overlay::Connect(form)) = app.overlay.as_mut() else {
        return;
    };
    let saved = match form.build() {
        Ok(saved) => saved,
        Err(error) => {
            form.error = Some(error);
            return;
        }
    };

    // The form stays up until the save has actually happened, so a credential
    // store that refuses does not take the typed connection string with it.
    match app.save_data_source(saved) {
        Ok(()) => app.overlay = None,
        Err(error) => {
            if let Some(Overlay::Connect(form)) = app.overlay.as_mut() {
                form.error = Some(error);
            }
        }
    }
}

pub fn execute(app: &mut App, command: Command) {
    match command {
        Command::NewConsole => app.new_console(),
        Command::CloseConsole => app.close_console(),
        Command::RunQuery => app.run_query(),
        Command::CancelQuery => app.cancel_query(),
        Command::NewDataSource => {
            app.overlay = Some(Overlay::Connect(ConnectForm::new(&app.config)));
        }
        Command::Connect(name) => app.bind_console(&name),
        Command::Disconnect(name) => app.disconnect(&name),
        Command::BindConsole(name) => app.bind_console(&name),
        Command::EditDataSource(name) => {
            if let Some(source) = app.config.get(&name).cloned() {
                app.overlay = Some(Overlay::Connect(ConnectForm::editing(
                    &name,
                    &source,
                    &app.config,
                )));
            }
        }
        Command::RemoveDataSource(name) => app.remove_data_source(&name),
        Command::Refresh => app.refresh_selected(),
        Command::Help => app.overlay = Some(Overlay::Help),
        Command::Quit => app.should_quit = true,
    }
}
