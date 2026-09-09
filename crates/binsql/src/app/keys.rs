//! Key handling.
//!
//! Three panes, each with its own bindings, plus a set that works anywhere.
//! Modifiers are chosen so nothing shadows ordinary typing in the editor: pane
//! movement is on Alt, not Ctrl+hjkl, because ⌃K belongs to the palette.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use tui_textarea::{Input, Key};

use super::overlay::{Command, ConnectForm, Field, Overlay, Palette};
use super::{App, Pane};

/// How far ⌃D and ⌃U move.
const HALF_PAGE: isize = 10;

pub fn handle(app: &mut App, key: KeyEvent) {
    if key.kind != KeyEventKind::Press {
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
        (KeyCode::Char('q'), true, _) => {
            app.should_quit = true;
        }
        (KeyCode::Char('k'), true, _) => {
            app.overlay = Some(Overlay::Palette(Palette::build(app)));
        }
        (KeyCode::Char('r'), true, _) => app.run_query(),
        (KeyCode::Char('t'), true, _) => app.new_console(),
        (KeyCode::Char('w'), true, _) => app.close_console(),
        (KeyCode::Char('n'), true, _) => {
            app.overlay = Some(Overlay::Connect(ConnectForm::new()));
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
        KeyCode::Char('n') => app.overlay = Some(Overlay::Connect(ConnectForm::new())),
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
    app.overlay = Some(Overlay::Connect(ConnectForm::editing(&name, &source)));
}

fn disconnect_selected_source(app: &mut App) {
    let Some(name) = app.context_source() else {
        return;
    };
    if app.sessions.contains_key(&name) {
        app.disconnect(&name);
    }
}

fn editor(app: &mut App, key: KeyEvent) {
    if key.code == KeyCode::Esc {
        app.focus = Pane::Explorer;
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
        KeyCode::Enter => app.overlay = Some(Overlay::Detail),
        KeyCode::Char('?') => app.overlay = Some(Overlay::Help),
        _ => {}
    }
}

fn overlay(app: &mut App, key: KeyEvent) {
    match app.overlay.as_mut() {
        Some(Overlay::Help) | Some(Overlay::Detail) => {
            if matches!(
                key.code,
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') | KeyCode::Char('?')
            ) {
                app.overlay = None;
            }
        }
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

fn save_form(app: &mut App) {
    let Some(Overlay::Connect(form)) = app.overlay.as_mut() else {
        return;
    };
    match form.build() {
        Ok((name, source)) => {
            let previous = form.editing.clone();
            app.overlay = None;
            if let Some(previous) = previous
                && previous != name
            {
                app.remove_data_source(&previous);
            }
            app.save_data_source(name, source);
        }
        Err(error) => form.error = Some(error),
    }
}

pub fn execute(app: &mut App, command: Command) {
    match command {
        Command::NewConsole => app.new_console(),
        Command::CloseConsole => app.close_console(),
        Command::RunQuery => app.run_query(),
        Command::NewDataSource => {
            app.overlay = Some(Overlay::Connect(ConnectForm::new()));
        }
        Command::Connect(name) => app.bind_console(&name),
        Command::Disconnect(name) => app.disconnect(&name),
        Command::BindConsole(name) => app.bind_console(&name),
        Command::EditDataSource(name) => {
            if let Some(source) = app.config.get(&name).cloned() {
                app.overlay = Some(Overlay::Connect(ConnectForm::editing(&name, &source)));
            }
        }
        Command::RemoveDataSource(name) => app.remove_data_source(&name),
        Command::Refresh => app.refresh_selected(),
        Command::Help => app.overlay = Some(Overlay::Help),
        Command::Quit => app.should_quit = true,
    }
}
