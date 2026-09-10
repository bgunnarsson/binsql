//! Drives the real application against a real database and renders it to a
//! test backend. This is the only way to assert that the thing people actually
//! look at is correct; unit tests on the model would pass with a blank screen.

use std::time::Duration;

use binsql::app::{App, Message, Pane};
use binsql::ui;
use binsql_core::{Backend, Config, DataSource};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tokio::sync::mpsc::UnboundedReceiver;

const WIDTH: u16 = 110;
const HEIGHT: u16 = 32;

/// One file per test — these run concurrently in the same process, so a shared
/// path would have them deleting each other's database mid-run.
fn fixture(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("binsql-ui-{name}-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    std::fs::write(&path, b"").expect("create database file");
    path
}

fn config(path: &std::path::Path) -> Config {
    let mut config = Config::default();
    config.set(
        "demo",
        DataSource {
            backend: Backend::Sqlite,
            dsn: path.display().to_string(),
            description: String::new(),
            read_only: false,
            open_on_start: true,
        },
    );
    config
}

/// Applies whatever background work has finished, waiting briefly for the first
/// message so a connect or a query has a chance to land.
async fn settle(app: &mut App, messages: &mut UnboundedReceiver<Message>) {
    if let Ok(Some(message)) = tokio::time::timeout(Duration::from_secs(5), messages.recv()).await {
        app.handle(message);
    }
    while let Ok(message) = messages.try_recv() {
        app.handle(message);
    }
    // A message often starts the next piece of work; give it a moment to reply.
    for _ in 0..8 {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let mut received = false;
        while let Ok(message) = messages.try_recv() {
            app.handle(message);
            received = true;
        }
        if !received {
            break;
        }
    }
}

fn render(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).expect("terminal");
    terminal
        .draw(|frame| ui::draw(frame, app))
        .expect("draw the whole layout");

    terminal
        .backend()
        .buffer()
        .content()
        .chunks(WIDTH as usize)
        .map(|row| {
            row.iter()
                .map(|cell| cell.symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn press(app: &mut App, code: KeyCode) {
    binsql::app::keys::handle(app, KeyEvent::new(code, KeyModifiers::NONE));
}

#[tokio::test]
async fn browses_a_database_and_shows_query_results() {
    let path = fixture("browse");

    // Seed through the core, so the test exercises the same path the UI does.
    {
        let session =
            binsql_core::Session::open("seed", config(&path).get("demo").unwrap().clone())
                .await
                .expect("open seed session");
        session
            .run(
                None,
                "CREATE TABLE artist (id INTEGER PRIMARY KEY, name TEXT NOT NULL, founded INTEGER)",
                None,
            )
            .await
            .expect("create table");
        session
            .run(None, "CREATE VIEW recent AS SELECT * FROM artist", None)
            .await
            .expect("create view");
        session
            .run(
                None,
                "INSERT INTO artist (name, founded) VALUES ('Portishead', 1991), ('Boards of Canada', 1986), ('Autechre', NULL)",
                None,
            )
            .await
            .expect("insert rows");
    }

    let (mut app, mut messages) = App::new(config(&path));
    app.open_startup_sources();
    settle(&mut app, &mut messages).await;

    // Connecting lists the catalogs and drops into the current one, which for
    // SQLite means `main` is expanded and its tables are loaded.
    let screen = render(&mut app);
    assert!(screen.contains("demo"), "data source missing:\n{screen}");
    assert!(screen.contains("main"), "catalog missing:\n{screen}");
    assert!(screen.contains("artist"), "table missing:\n{screen}");
    assert!(screen.contains("Tables"), "group missing:\n{screen}");
    assert!(screen.contains("Views"), "views group missing:\n{screen}");

    // Walk down to the table and open it.
    let mut guard = 0;
    while !matches!(
        app.tree
            .selected_id()
            .and_then(|id| app.tree.find(id))
            .map(|node| node.kind.clone()),
        Some(binsql::app::tree::NodeKind::Object { ref object }) if object.name == "artist"
    ) {
        press(&mut app, KeyCode::Char('j'));
        guard += 1;
        assert!(
            guard < 40,
            "never reached the artist table:\n{}",
            render(&mut app)
        );
    }

    press(&mut app, KeyCode::Enter);
    settle(&mut app, &mut messages).await;

    let screen = render(&mut app);
    assert!(
        screen.contains("Portishead"),
        "query results missing:\n{screen}"
    );
    assert!(
        screen.contains("Boards of Canada"),
        "second row missing:\n{screen}"
    );
    assert!(screen.contains("NULL"), "null marker missing:\n{screen}");
    assert!(
        screen.contains("3 rows"),
        "row count missing from the status line:\n{screen}"
    );
    assert_eq!(app.focus, Pane::Results, "focus should follow the results");

    println!("\n{screen}\n");

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn help_and_palette_open_over_the_layout() {
    let path = fixture("overlays");
    let (mut app, _messages) = App::new(config(&path));

    binsql::app::keys::handle(
        &mut app,
        KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
    );
    let screen = render(&mut app);
    assert!(screen.contains("Commands"), "palette missing:\n{screen}");
    assert!(screen.contains("New console"), "command missing:\n{screen}");

    press(&mut app, KeyCode::Esc);
    binsql::app::keys::handle(&mut app, KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
    let screen = render(&mut app);
    assert!(screen.contains("Help"), "help missing:\n{screen}");
    assert!(
        screen.contains("Command palette"),
        "binding missing:\n{screen}"
    );

    println!("\n{screen}\n");

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn the_connection_form_reports_what_is_wrong() {
    let path = fixture("form");
    let (mut app, _messages) = App::new(config(&path));

    binsql::app::keys::handle(
        &mut app,
        KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
    );
    // Folder, name, then connection string.
    press(&mut app, KeyCode::Tab);
    for ch in "prod".chars() {
        press(&mut app, KeyCode::Char(ch));
    }
    press(&mut app, KeyCode::Tab);
    for ch in "nonsense".chars() {
        press(&mut app, KeyCode::Char(ch));
    }
    press(&mut app, KeyCode::Enter);

    let screen = render(&mut app);
    assert!(
        screen.contains("Could not tell the driver"),
        "form should refuse an unrecognisable DSN:\n{screen}"
    );

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn ctrl_q_always_quits() {
    // Raw mode disables ISIG, so ⌃C is gone and ⌃Q is the only way out. Every
    // modal used to swallow it, which trapped people inside the program.
    let path = fixture("quit");
    let ctrl_q = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL);

    let openers: [(&str, KeyEvent); 4] = [
        (
            "nothing open",
            KeyEvent::new(KeyCode::Null, KeyModifiers::NONE),
        ),
        (
            "command palette",
            KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
        ),
        ("help", KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE)),
        (
            "connection form",
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
        ),
    ];

    for (what, opener) in openers {
        let (mut app, _messages) = App::new(config(&path));
        binsql::app::keys::handle(&mut app, opener);
        binsql::app::keys::handle(&mut app, ctrl_q);
        assert!(app.should_quit, "⌃Q did not quit with {what} open");
    }

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn help_closes_on_any_key_as_it_claims() {
    let path = fixture("helpclose");
    let (mut app, _messages) = App::new(config(&path));

    binsql::app::keys::handle(&mut app, KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
    assert!(render(&mut app).contains("Any key closes this"));

    press(&mut app, KeyCode::Char('x'));
    assert!(
        !render(&mut app).contains("Any key closes this"),
        "help should have closed"
    );

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn enter_shows_the_whole_record_stacked() {
    let path = fixture("rowdetail");
    {
        let session =
            binsql_core::Session::open("seed", config(&path).get("demo").unwrap().clone())
                .await
                .expect("open seed session");
        session
            .run(
                None,
                "CREATE TABLE audit (id INTEGER PRIMARY KEY, delta INTEGER, entity TEXT, \
                 comment TEXT, reviewed INTEGER)",
                None,
            )
            .await
            .expect("create table");
        session
            .run(
                None,
                "INSERT INTO audit (delta, entity, comment, reviewed) VALUES \
                 (-1, 'User', '\"brynjolfur@vettvangur.is\" <brynjolfur@vettvangur.is> \
                 changed the publication schedule for the shipping page', NULL), \
                 (1, 'User', '\"binni@vettvangur.is\" <binni@vettvangur.is>', 1)",
                None,
            )
            .await
            .expect("insert rows");
    }

    let (mut app, mut messages) = App::new(config(&path));
    app.open_startup_sources();
    settle(&mut app, &mut messages).await;

    let mut guard = 0;
    while !matches!(
        app.tree.selected_id().and_then(|id| app.tree.find(id)).map(|n| n.kind.clone()),
        Some(binsql::app::tree::NodeKind::Object { ref object }) if object.name == "audit"
    ) {
        press(&mut app, KeyCode::Char('j'));
        guard += 1;
        assert!(guard < 40, "never reached the audit table");
    }
    press(&mut app, KeyCode::Enter);
    settle(&mut app, &mut messages).await;

    // Open the record viewer on the first row.
    press(&mut app, KeyCode::Enter);
    let screen = render(&mut app);
    println!("\n{screen}\n");

    // Every column is present, labelled, not just the one under the cursor.
    for column in ["id", "delta", "entity", "comment", "reviewed"] {
        assert!(
            screen.contains(column),
            "column {column} missing:\n{screen}"
        );
    }
    assert!(screen.contains("Row 1 of 2"), "title missing:\n{screen}");
    // The wrap point moves with the box width, so assert on the tail of the
    // value: if the end of it is on screen, nothing was truncated away.
    assert!(
        screen.contains("shipping page"),
        "the end of the long value should be visible, not truncated:\n{screen}"
    );
    assert!(screen.contains("NULL"), "null field missing:\n{screen}");

    // Right steps to the next record without closing.
    press(&mut app, KeyCode::Right);
    let screen = render(&mut app);
    assert!(
        screen.contains("Row 2 of 2"),
        "did not step record:\n{screen}"
    );
    assert!(screen.contains("binni@vettvangur.is"), "{screen}");

    // Esc closes it and leaves the grid on the row we walked to.
    press(&mut app, KeyCode::Esc);
    assert!(app.overlay.is_none(), "Esc should close the viewer");
    assert_eq!(
        app.console().grid().expect("grid").row,
        1,
        "stepping records should move the grid cursor with it"
    );

    let _ = std::fs::remove_file(&path);
}

fn mouse(app: &mut App, kind: MouseEventKind, (column, row): (u16, u16)) {
    binsql::app::mouse::handle(
        app,
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        },
    );
}

/// A press, a drag and a release, in whatever direction the seam moves.
fn drag(app: &mut App, from: (u16, u16), to: (u16, u16)) {
    mouse(app, MouseEventKind::Down(MouseButton::Left), from);
    mouse(app, MouseEventKind::Drag(MouseButton::Left), to);
    mouse(app, MouseEventKind::Up(MouseButton::Left), to);
}

fn click(app: &mut App, at: (u16, u16)) {
    mouse(app, MouseEventKind::Down(MouseButton::Left), at);
    mouse(app, MouseEventKind::Up(MouseButton::Left), at);
}

fn wheel(app: &mut App, at: (u16, u16), notches: i32) {
    let kind = if notches < 0 {
        MouseEventKind::ScrollUp
    } else {
        MouseEventKind::ScrollDown
    };
    for _ in 0..notches.abs() {
        mouse(app, kind, at);
    }
}

/// The one line of the screen the query editor's text starts on.
fn text_top_line(app: &mut App) -> String {
    let text = app.panes.text;
    let screen = render(app);
    screen
        .lines()
        .nth(text.y as usize)
        .unwrap_or_default()
        .to_string()
}

/// The query editor scrolls, and a click afterwards lands on the line it looks
/// like it is on.
///
/// tui-textarea keeps its viewport private, so binsql keeps a copy to turn a
/// click into a position in the text. This is what proves the two agree: the
/// assertion on the cursor reads binsql's copy, and the assertion on the
/// screen reads what the widget actually drew.
#[tokio::test]
async fn the_editor_scrolls_and_a_click_lands_where_it_looks() {
    let path = fixture("editorscroll");
    let (mut app, _messages) = App::new(config(&path));

    // Comfortably more lines than the pane can show.
    let sql: String = (0..40).map(|i| format!("SELECT {i};\n")).collect();
    app.console_mut().set_sql(&sql);
    render(&mut app);

    let text = app.panes.text;
    assert!(text.height > 2 && text.height < 40, "a scrolling pane");

    // Writing left the cursor at the end, so the view sits at the bottom.
    let top = text_top_line(&mut app);
    // The screen line carries the sidebar and the borders too, so take the
    // statement out of the middle of it.
    let first_visible: usize = top
        .split("SELECT ")
        .nth(1)
        .and_then(|rest| rest.split(';').next())
        .and_then(|n| n.trim().parse().ok())
        .unwrap_or_else(|| panic!("unexpected top line {top:?}"));
    assert!(first_visible > 0, "the pane should have scrolled: {top:?}");

    // A click on the top row is the line the top row is showing.
    click(&mut app, (text.x + 2, text.y));
    assert_eq!(
        app.console().editor.cursor().0,
        first_visible,
        "clicking the top row put the cursor on the wrong line"
    );

    // Wheel up, and the same click is three lines earlier — in the widget's
    // own rendering as much as in ours.
    wheel(&mut app, (text.x + 2, text.y + 1), -1);
    render(&mut app);
    let top = text_top_line(&mut app);
    assert!(
        top.contains(&format!("SELECT {};", first_visible - 3)),
        "the wheel should have scrolled the widget: {top:?}"
    );

    click(&mut app, (text.x + 2, text.y));
    assert_eq!(
        app.console().editor.cursor().0,
        first_visible - 3,
        "our copy of the viewport drifted from the widget's"
    );

    let _ = std::fs::remove_file(&path);
}

/// Select all, delete to the head of the line, and a mouse selection that a
/// backspace removes.
#[tokio::test]
async fn the_query_editor_edits_like_an_editor() {
    let path = fixture("editing");
    let (mut app, _messages) = App::new(config(&path));

    let ctrl = |app: &mut App, ch| {
        binsql::app::keys::handle(app, KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL))
    };

    // Select all, then type: the selection is replaced, not pushed along.
    app.console_mut().set_sql("SELECT 1;\nSELECT 2;");
    app.focus = Pane::Editor;
    render(&mut app);
    ctrl(&mut app, 'a');
    assert!(app.console().editor.is_selecting(), "⌃A should select");
    press(&mut app, KeyCode::Char('X'));
    assert_eq!(
        app.console().sql(),
        "X",
        "typing should replace a selection"
    );

    // ⌘⌫ where the terminal sends it, ⌃U where it does not — both cut back to
    // the head of the line.
    for modifier in [KeyModifiers::SUPER, KeyModifiers::CONTROL] {
        app.console_mut().set_sql("SELECT * FROM artist");
        binsql::app::keys::handle(&mut app, KeyEvent::new(KeyCode::Backspace, modifier));
        assert_eq!(
            app.console().sql(),
            "",
            "backspace with {modifier:?} should clear the line"
        );
    }
    app.console_mut().set_sql("SELECT * FROM artist");
    ctrl(&mut app, 'u');
    assert_eq!(app.console().sql(), "", "⌃U should clear the line");

    // Undo and redo, in both spellings. ⌃Z and ⌃Y are the textarea's own;
    // ⌘Z and ⌘⇧Z are ours, because it has no idea what ⌘ is.
    let chord = |app: &mut App, code, modifiers| {
        binsql::app::keys::handle(app, KeyEvent::new(code, modifiers))
    };
    for (undo, redo) in [
        (
            (KeyCode::Char('z'), KeyModifiers::CONTROL),
            (KeyCode::Char('y'), KeyModifiers::CONTROL),
        ),
        (
            (KeyCode::Char('z'), KeyModifiers::SUPER),
            (
                KeyCode::Char('Z'),
                KeyModifiers::SUPER | KeyModifiers::SHIFT,
            ),
        ),
    ] {
        app.console_mut().set_sql("SELECT 1");
        press(&mut app, KeyCode::Char('!'));
        assert_eq!(app.console().sql(), "SELECT 1!");

        chord(&mut app, undo.0, undo.1);
        assert_eq!(
            app.console().sql(),
            "SELECT 1",
            "undo failed for {:?}",
            undo.1
        );

        chord(&mut app, redo.0, redo.1);
        assert_eq!(
            app.console().sql(),
            "SELECT 1!",
            "redo failed for {:?}",
            redo.1
        );
    }

    // A ⌘ chord with nothing bound to it must not type its letter.
    app.console_mut().set_sql("SELECT 1");
    chord(&mut app, KeyCode::Char('s'), KeyModifiers::SUPER);
    assert_eq!(
        app.console().sql(),
        "SELECT 1",
        "an unbound ⌘ chord should not reach the text"
    );

    // Drag across the first word, then delete it.
    app.console_mut().set_sql("SELECT everything");
    render(&mut app);
    let text = app.panes.text;
    drag(&mut app, (text.x, text.y), (text.x + 7, text.y));
    assert!(
        app.console().editor.is_selecting(),
        "a drag should be selecting"
    );
    press(&mut app, KeyCode::Backspace);
    assert_eq!(
        app.console().sql(),
        "everything",
        "backspace should remove what the mouse selected"
    );

    let _ = std::fs::remove_file(&path);
}

fn double_click(app: &mut App, at: (u16, u16)) {
    click(app, at);
    click(app, at);
}

/// The row a label is drawn on, so a test can aim at what it can see.
fn row_of(app: &mut App, label: &str) -> u16 {
    let screen = render(app);
    screen
        .lines()
        .position(|line| line.contains(label))
        .unwrap_or_else(|| panic!("{label} is not on screen:\n{screen}")) as u16
}

/// Double-clicking down the sidebar opens each thing in turn, the way Enter
/// does — a catalog expands, a table runs.
#[tokio::test]
async fn double_clicking_the_sidebar_opens_things() {
    let path = fixture("doubleclick");
    {
        let session =
            binsql_core::Session::open("seed", config(&path).get("demo").unwrap().clone())
                .await
                .expect("open seed session");
        session
            .run(None, "CREATE TABLE artist (id INTEGER, name TEXT)", None)
            .await
            .expect("create table");
        session
            .run(None, "INSERT INTO artist VALUES (1,'Portishead')", None)
            .await
            .expect("insert row");
    }

    let (mut app, mut messages) = App::new(config(&path));
    app.open_startup_sources();
    settle(&mut app, &mut messages).await;

    // Connecting leaves the group open, so a double click shuts it — the same
    // toggle Enter is, on a node that has something to toggle.
    let tables = row_of(&mut app, "Tables");
    let column = app.panes.tree.x + 8;
    double_click(&mut app, (column, tables));

    let screen = render(&mut app);
    assert!(
        !screen.contains("artist"),
        "double-clicking an open group should close it:\n{screen}"
    );

    double_click(&mut app, (column, tables));
    let screen = render(&mut app);
    assert!(
        screen.contains("artist"),
        "and again should open it:\n{screen}"
    );

    // A table is not a toggle: opening it runs a query for it.
    let artist = row_of(&mut app, "artist");
    double_click(&mut app, (column + 4, artist));
    settle(&mut app, &mut messages).await;

    let screen = render(&mut app);
    println!("\n{screen}\n");
    assert!(
        screen.contains("Portishead"),
        "double-clicking a table should open it:\n{screen}"
    );
    assert_eq!(app.focus, Pane::Results, "focus should follow the results");

    let _ = std::fs::remove_file(&path);
}

/// Two presses far enough apart in space are two clicks, not one gesture.
#[tokio::test]
async fn a_single_click_only_selects() {
    let path = fixture("singleclick");
    let (mut app, _messages) = App::new(config(&path));
    render(&mut app);

    // The source is registered but not connected, and one click must not
    // connect it — that is what the second click is for.
    let tree = app.panes.tree;
    click(&mut app, (tree.x + 2, tree.y));
    assert!(app.sessions.is_empty(), "a single click should only select");

    // Two clicks on different rows are not a double click either.
    click(&mut app, (tree.x + 2, tree.y));
    click(&mut app, (tree.x + 2, tree.y + 1));
    assert!(
        app.sessions.is_empty(),
        "clicks on different rows are not a double click"
    );

    let _ = std::fs::remove_file(&path);
}

/// A click takes focus, and lands on the thing under it.
#[tokio::test]
async fn clicking_a_pane_focuses_it() {
    let path = fixture("clickfocus");
    {
        let session =
            binsql_core::Session::open("seed", config(&path).get("demo").unwrap().clone())
                .await
                .expect("open seed session");
        session
            .run(None, "CREATE TABLE artist (id INTEGER, name TEXT)", None)
            .await
            .expect("create table");
        session
            .run(
                None,
                "INSERT INTO artist VALUES (1,'a'),(2,'b'),(3,'c')",
                None,
            )
            .await
            .expect("insert rows");
    }

    let (mut app, mut messages) = App::new(config(&path));
    app.open_startup_sources();
    settle(&mut app, &mut messages).await;
    app.console_mut().set_sql("SELECT * FROM artist");
    app.run_query();
    settle(&mut app, &mut messages).await;
    render(&mut app);

    // The editor.
    let text = app.panes.text;
    click(&mut app, (text.x + 3, text.y));
    assert_eq!(app.focus, Pane::Editor, "a click should focus the editor");
    assert_eq!(app.console().editor.cursor(), (0, 3), "and land the cursor");

    // The grid: the third row, and a column past the first.
    render(&mut app);
    let grid = app.panes.grid;
    let column = grid.x + app.console().grid().expect("grid").gutter() + 5;
    click(&mut app, (column, grid.y + 3));
    assert_eq!(app.focus, Pane::Results, "a click should focus the results");
    assert_eq!(app.console().grid().expect("grid").row, 2, "the third row");
    assert_eq!(
        app.console().grid().expect("grid").column,
        1,
        "the second column"
    );

    // The tree.
    render(&mut app);
    let tree = app.panes.tree;
    click(&mut app, (tree.x + 2, tree.y + 1));
    assert_eq!(app.focus, Pane::Explorer, "a click should focus the tree");
    assert_eq!(app.tree.selected, 1, "the second visible row");

    let _ = std::fs::remove_file(&path);
}

/// The seam between the sidebar and the workspace, dragged sideways.
fn drag_divider(app: &mut App, from: u16, to: u16) {
    drag(app, (from, 4), (to, 4));
}

/// The seam between the query pane and the results, dragged up or down.
fn drag_split(app: &mut App, from: u16, to: u16) {
    let column = app.panes.editor.x + 4;
    drag(app, (column, from), (column, to));
}

#[tokio::test]
async fn the_sidebar_widens_by_dragging_its_edge() {
    // A schema of long table names does not fit the width that suits a schema
    // of short ones, and the layout cannot know which it is looking at.
    let path = fixture("divider");
    const LONG: &str = "_NestedContentMigrationBackup";
    {
        let session =
            binsql_core::Session::open("seed", config(&path).get("demo").unwrap().clone())
                .await
                .expect("open seed session");
        session
            .run(None, &format!("CREATE TABLE {LONG} (id INTEGER)"), None)
            .await
            .expect("create table");
    }

    let (mut app, mut messages) = App::new(config(&path));
    app.open_startup_sources();
    settle(&mut app, &mut messages).await;

    // The renderer is what records where the panes landed, so nothing can be
    // dragged before the first frame.
    let screen = render(&mut app);
    assert!(
        !screen.contains(LONG),
        "the default width should not fit this name, or the test proves nothing:\n{screen}"
    );
    let before = app.panes.explorer.width;
    let edge = app.panes.explorer.right() - 1;

    drag_divider(&mut app, edge, edge + 14);
    let screen = render(&mut app);
    println!("\n{screen}\n");
    assert_eq!(
        app.panes.explorer.width,
        before + 14,
        "the sidebar should follow the drag"
    );
    // Which is the whole point: the name that did not fit now does.
    assert!(
        screen.contains(LONG),
        "a widened sidebar should show the whole name:\n{screen}"
    );

    // It only ever widens: the default is already what fits ordinary names.
    let edge = app.panes.explorer.right() - 1;
    drag_divider(&mut app, edge, 4);
    render(&mut app);
    assert_eq!(
        app.panes.explorer.width, before,
        "dragging in should stop at the default width"
    );

    // A press that missed the seam is not the start of a drag.
    let edge = app.panes.explorer.right() - 1;
    drag_divider(&mut app, edge.saturating_sub(6), edge + 20);
    render(&mut app);
    assert_eq!(
        app.panes.explorer.width, before,
        "a press away from the seam should not resize anything"
    );

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn the_results_pane_grows_by_dragging_the_split() {
    // A query being written wants room the same query's results do not, so
    // this one moves both ways rather than only outwards.
    let path = fixture("split");
    let (mut app, _messages) = App::new(config(&path));

    render(&mut app);
    let editor = app.panes.editor.height;
    let results = app.panes.results.height;
    let seam = app.panes.editor.bottom() - 1;

    // Up: a smaller query pane, a bigger grid.
    drag_split(&mut app, seam, seam - 6);
    let screen = render(&mut app);
    println!("\n{screen}\n");
    assert_eq!(
        app.panes.editor.height,
        editor - 6,
        "the query pane shrinks"
    );
    assert_eq!(
        app.panes.results.height,
        results + 6,
        "and the grid takes what it gave up"
    );

    // Down again, past where it started.
    let seam = app.panes.editor.bottom() - 1;
    drag_split(&mut app, seam, seam + 10);
    render(&mut app);
    assert_eq!(app.panes.editor.height, editor + 4, "the query pane grows");

    // Neither pane can be squeezed out, from either end.
    let seam = app.panes.editor.bottom() - 1;
    drag_split(&mut app, seam, 0);
    render(&mut app);
    assert!(
        app.panes.editor.height >= 3,
        "the query pane kept a line to write on: {}",
        app.panes.editor.height
    );

    let seam = app.panes.editor.bottom() - 1;
    drag_split(&mut app, seam, HEIGHT);
    render(&mut app);
    assert!(
        app.panes.results.height >= 5,
        "the grid kept room for a row: {}",
        app.panes.results.height
    );

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn a_dragged_sidebar_still_leaves_room_to_work() {
    let path = fixture("dividerclamp");
    let (mut app, _messages) = App::new(config(&path));
    render(&mut app);

    // Dragged past the right-hand edge of the terminal.
    let edge = app.panes.explorer.right() - 1;
    drag_divider(&mut app, edge, WIDTH + 40);
    render(&mut app);
    assert!(
        app.panes.explorer.width < WIDTH,
        "the workspace should keep a usable width: {}",
        app.panes.explorer.width
    );

    // And a window that shrinks re-fits it rather than stranding it.
    let mut terminal = Terminal::new(TestBackend::new(60, 20)).expect("terminal");
    terminal
        .draw(|frame| ui::draw(frame, &mut app))
        .expect("draw");
    assert!(
        app.panes.explorer.width < 60,
        "the sidebar should fit the smaller terminal: {}",
        app.panes.explorer.width
    );

    let _ = std::fs::remove_file(&path);
}

/// The header answers one question — where ⌃R sends this SQL — so what it
/// shows has to survive both a connection and a narrow terminal.
#[tokio::test]
async fn the_header_says_where_the_query_will_run() {
    let path = fixture("header");
    let (mut app, mut messages) = App::new(config(&path));

    // Before anything is bound there is nothing better to say than the name of
    // the program.
    let screen = render(&mut app);
    let header = screen.lines().next().expect("a header").to_string();
    assert!(header.contains("binsql"), "header: {header}");
    assert!(header.contains("no data source"), "header: {header}");

    app.open_startup_sources();
    settle(&mut app, &mut messages).await;

    let screen = render(&mut app);
    let header = screen.lines().next().expect("a header").to_string();
    println!("\n{header}\n");
    assert!(header.contains("demo"), "the source is missing: {header}");
    assert!(header.contains("main"), "the catalog is missing: {header}");
    // The engine names its version, and the host says which server that is —
    // two connections called `prod` are otherwise identical on screen.
    assert!(
        header.contains("SQLite 3."),
        "the engine version is missing: {header}"
    );
    let file = path.file_name().unwrap().to_string_lossy().to_string();
    assert!(header.contains(&file), "the host is missing: {header}");
    // The stored connection string is not the host, and must not be shown.
    assert!(
        !header.contains(&path.display().to_string()),
        "the header should not print the connection string: {header}"
    );

    // Squeezed, the detail goes before the identity does.
    let mut terminal = Terminal::new(TestBackend::new(44, 12)).expect("terminal");
    terminal
        .draw(|frame| ui::draw(frame, &mut app))
        .expect("draw");
    let narrow: String = (0..44)
        .map(|x| terminal.backend().buffer()[(x, 0)].symbol())
        .collect();
    println!("[{narrow}]");
    assert!(
        narrow.contains("demo"),
        "the source should outlast the detail: {narrow}"
    );
    assert!(
        !narrow.contains("SQLite 3."),
        "the detail should have given way: {narrow}"
    );

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn the_header_marks_a_read_only_connection() {
    let path = fixture("readonly");
    let mut config = Config::default();
    config.set(
        "prod",
        DataSource {
            backend: Backend::Sqlite,
            dsn: path.display().to_string(),
            description: String::new(),
            read_only: true,
            open_on_start: true,
        },
    );

    let (mut app, mut messages) = App::new(config);
    app.open_startup_sources();
    settle(&mut app, &mut messages).await;

    let header = render(&mut app)
        .lines()
        .next()
        .expect("a header")
        .to_string();
    println!("\n{header}\n");
    assert!(
        header.contains("read-only"),
        "a connection that refuses writes should say so: {header}"
    );

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn a_field_opens_in_full_from_the_record() {
    // The record viewer wraps every value into one narrow column. A JSON
    // document stored on a single line is unreadable that way, so Enter opens
    // the selected field on its own, re-indented.
    let path = fixture("fieldvalue");
    let payload = r#"{"id":42,"tags":["alpha","beta"],"note":"a value long enough that the grid can only ever show the front of it"}"#;
    {
        let session =
            binsql_core::Session::open("seed", config(&path).get("demo").unwrap().clone())
                .await
                .expect("open seed session");
        session
            .run(
                None,
                "CREATE TABLE doc (id INTEGER PRIMARY KEY, payload TEXT)",
                None,
            )
            .await
            .expect("create table");
        session
            .run(
                None,
                &format!("INSERT INTO doc (payload) VALUES ('{payload}')"),
                None,
            )
            .await
            .expect("insert row");
    }

    let (mut app, mut messages) = App::new(config(&path));
    app.open_startup_sources();
    settle(&mut app, &mut messages).await;

    let mut guard = 0;
    while !matches!(
        app.tree.selected_id().and_then(|id| app.tree.find(id)).map(|n| n.kind.clone()),
        Some(binsql::app::tree::NodeKind::Object { ref object }) if object.name == "doc"
    ) {
        press(&mut app, KeyCode::Char('j'));
        guard += 1;
        assert!(guard < 40, "never reached the doc table");
    }
    press(&mut app, KeyCode::Enter);
    settle(&mut app, &mut messages).await;

    // Open the record, then move down a field: the selection is the grid's
    // cursor column, so the modal's title follows it.
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char('j'));
    let screen = render(&mut app);
    assert!(
        screen.contains("payload TEXT"),
        "the title should name the selected field:\n{screen}"
    );
    assert_eq!(
        app.console().grid().expect("grid").column,
        1,
        "moving in the record should move the grid cursor with it"
    );

    press(&mut app, KeyCode::Enter);
    let screen = render(&mut app);
    println!("\n{screen}\n");

    // Re-indented: each key on its own line, and the tail of the long note
    // present rather than cut off.
    assert!(
        screen.contains("\"tags\": ["),
        "JSON should be re-indented:\n{screen}"
    );
    assert!(
        screen.contains("\"alpha\","),
        "array members should be on their own lines:\n{screen}"
    );
    assert!(
        screen.contains("front of it"),
        "the end of the value should be readable:\n{screen}"
    );
    assert!(
        screen.contains("chars"),
        "the border should size the value:\n{screen}"
    );

    // Esc steps back to the record rather than all the way out.
    press(&mut app, KeyCode::Esc);
    let screen = render(&mut app);
    assert!(
        screen.contains("Row 1 of 1"),
        "Esc should return to the record:\n{screen}"
    );
    press(&mut app, KeyCode::Esc);
    assert!(app.overlay.is_none(), "Esc should then close the record");

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn the_record_viewer_is_sized_to_the_record() {
    // A handful of short fields used to open a modal covering 80% of the
    // screen, most of it empty.
    let path = fixture("rowsize");
    {
        let session =
            binsql_core::Session::open("seed", config(&path).get("demo").unwrap().clone())
                .await
                .expect("open seed session");
        session
            .run(
                None,
                "CREATE TABLE small (a INTEGER, b INTEGER, c TEXT)",
                None,
            )
            .await
            .expect("create table");
        session
            .run(None, "INSERT INTO small VALUES (1, 2, 'ok')", None)
            .await
            .expect("insert row");
    }

    let (mut app, mut messages) = App::new(config(&path));
    app.open_startup_sources();
    settle(&mut app, &mut messages).await;

    let mut guard = 0;
    while !matches!(
        app.tree.selected_id().and_then(|id| app.tree.find(id)).map(|n| n.kind.clone()),
        Some(binsql::app::tree::NodeKind::Object { ref object }) if object.name == "small"
    ) {
        press(&mut app, KeyCode::Char('j'));
        guard += 1;
        assert!(guard < 40, "never reached the small table");
    }
    press(&mut app, KeyCode::Enter);
    settle(&mut app, &mut messages).await;
    press(&mut app, KeyCode::Enter);

    let screen = render(&mut app);
    println!("\n{screen}\n");

    let rows: Vec<&str> = screen.lines().collect();
    let top = rows
        .iter()
        .position(|line| line.contains("Row 1 of 1"))
        .expect("record viewer drawn");
    let bottom = rows
        .iter()
        .skip(top)
        .position(|line| line.contains('╰'))
        .expect("record viewer closed")
        + top;

    // Three fields, a footer and two borders.
    let height = bottom - top + 1;
    assert_eq!(
        height, 6,
        "expected a six-line box, got {height}:\n{screen}"
    );
    assert!(
        height < (HEIGHT as usize) / 2,
        "the box should not dominate the screen"
    );

    // Measure the box itself, not the whole terminal line it sits on.
    let box_width = box_width(rows[top]);
    assert!(
        box_width < WIDTH as usize / 2,
        "short values should give a narrow box, got {box_width}:\n{screen}"
    );

    let _ = std::fs::remove_file(&path);
}

/// The width of the overlay drawn on `line`, from its opening corner to the
/// matching closing one. The rest of the line is whatever it was drawn over.
fn box_width(line: &str) -> usize {
    let chars: Vec<char> = line.chars().collect();
    let start = chars
        .iter()
        .position(|c| *c == '\u{256d}')
        .expect("an opening corner");
    let end = chars[start..]
        .iter()
        .position(|c| *c == '\u{256e}')
        .expect("a closing corner")
        + start;
    end - start + 1
}

/// Relative luminance, per WCAG.
fn luminance(colour: ratatui::style::Color) -> f64 {
    let ratatui::style::Color::Rgb(r, g, b) = colour else {
        return 0.0;
    };
    let channel = |v: u8| {
        let v = f64::from(v) / 255.0;
        if v <= 0.039_28 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

fn contrast(fg: ratatui::style::Color, bg: ratatui::style::Color) -> f64 {
    let (a, b) = (luminance(fg), luminance(bg));
    let (lighter, darker) = if a > b { (a, b) } else { (b, a) };
    (lighter + 0.05) / (darker + 0.05)
}

#[tokio::test]
async fn the_header_and_status_line_are_legible() {
    // The hint strip was once `dim` on `surface`, a contrast of about 1.6 —
    // present on screen and unreadable. Text with a background dark enough to
    // hide it is a bug, not a style.
    const FLOOR: f64 = 3.0;

    let path = fixture("contrast");
    let (mut app, _messages) = App::new(config(&path));

    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).expect("terminal");
    terminal
        .draw(|frame| ui::draw(frame, &mut app))
        .expect("draw");

    let buffer = terminal.backend().buffer();
    let rows = [0u16, HEIGHT - 1];

    for y in rows {
        for x in 0..WIDTH {
            let cell = &buffer[(x, y)];
            let symbol = cell.symbol();
            if symbol.trim().is_empty() {
                continue;
            }
            // The powerline arrows are solid shapes, not glyphs read against a
            // background: their two colours are the segments either side of
            // them by design, so contrast does not apply.
            if symbol == "\u{e0b0}" || symbol == "\u{e0b2}" {
                continue;
            }
            let ratio = contrast(cell.fg, cell.bg);
            assert!(
                ratio >= FLOOR,
                "row {y} col {x} {symbol:?} has contrast {ratio:.2} \
                 (fg {:?} on bg {:?}), below {FLOOR}",
                cell.fg,
                cell.bg,
            );
        }
    }

    let _ = std::fs::remove_file(&path);
}

/// A config with two folders, each holding a connection of the same name.
fn foldered(dir: &std::path::Path) -> Config {
    let raw = format!(
        r#"{{
            "default": "eimskip/local",
            "connections": {{
                "eimskip": {{
                    "local": {{ "driver": "sqlite", "dsn": "{a}", "open_on_start": true }},
                    "prod":  {{ "driver": "sqlite", "dsn": "{a}", "readonly": true }}
                }},
                "osar": {{
                    "prod": {{ "driver": "sqlite", "dsn": "{b}" }}
                }},
                "scratch": {{ "driver": "sqlite", "dsn": "{b}" }}
            }}
        }}"#,
        a = dir.join("a.db").display(),
        b = dir.join("b.db").display(),
    );
    serde_json::from_str(&raw).expect("config parses")
}

#[tokio::test]
async fn folders_group_the_sidebar() {
    let dir = std::env::temp_dir().join(format!("binsql-folders-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    for name in ["a.db", "b.db"] {
        std::fs::write(dir.join(name), b"").expect("create database");
    }

    let (mut app, mut messages) = App::new(foldered(&dir));
    app.open_startup_sources();
    settle(&mut app, &mut messages).await;

    let screen = render(&mut app);
    println!("\n{screen}\n");
    for expected in ["eimskip", "osar", "scratch"] {
        assert!(screen.contains(expected), "{expected} missing:\n{screen}");
    }

    // Two connections named `prod` coexist because their folders differ.
    assert!(app.config.get("eimskip/prod").is_some());
    assert!(app.config.get("osar/prod").is_some());

    // The tree shows leaf names under their folder, not qualified ones.
    assert!(
        !screen.contains("eimskip/prod"),
        "the sidebar should not repeat the folder on each row:\n{screen}"
    );

    // `default` was qualified, and that is the session that opened.
    assert!(
        app.sessions.contains_key("eimskip/local"),
        "expected eimskip/local to be connected, got {:?}",
        app.sessions.keys().collect::<Vec<_>>()
    );

    // Four connections across three top-level entries.
    assert_eq!(app.config.len(), 4);
    assert_eq!(app.tree.roots.len(), 3);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn folders_start_closed() {
    let dir = std::env::temp_dir().join(format!("binsql-closed-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    for name in ["a.db", "b.db"] {
        std::fs::write(dir.join(name), b"").expect("create database");
    }

    // Nothing opens on its own here: no `open_on_start`, and the default is
    // ambiguous on purpose.
    let raw = format!(
        r#"{{
            "default": "prod",
            "connections": {{
                "eimskip": {{
                    "local": {{ "driver": "sqlite", "dsn": "{a}" }},
                    "prod":  {{ "driver": "sqlite", "dsn": "{a}" }}
                }},
                "osar": {{ "prod": {{ "driver": "sqlite", "dsn": "{b}" }} }}
            }}
        }}"#,
        a = dir.join("a.db").display(),
        b = dir.join("b.db").display(),
    );
    let config: Config = serde_json::from_str(&raw).expect("config parses");

    let (mut app, _messages) = App::new(config);
    let screen = render(&mut app);
    println!("\n{screen}\n");

    assert!(screen.contains("eimskip"), "folders missing:\n{screen}");
    assert!(screen.contains("osar"), "folders missing:\n{screen}");
    assert!(
        !screen.contains("local"),
        "a closed folder should not show what is inside it:\n{screen}"
    );
    assert_eq!(
        app.tree.visible().len(),
        2,
        "only the two folders should be visible"
    );

    // Opening one shows its connections and leaves the other alone.
    press(&mut app, KeyCode::Char('l'));
    let screen = render(&mut app);
    assert!(
        screen.contains("local"),
        "expanding should reveal:\n{screen}"
    );
    assert_eq!(app.tree.visible().len(), 4, "one folder open, one closed");

    let _ = std::fs::remove_dir_all(&dir);
}

/// The colour of the cell at a position, as the buffer holds it.
fn cell_colours(app: &mut App, x: u16, y: u16) -> (ratatui::style::Color, ratatui::style::Color) {
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).expect("terminal");
    terminal.draw(|frame| ui::draw(frame, app)).expect("draw");
    let cell = &terminal.backend().buffer()[(x, y)];
    (cell.fg, cell.bg)
}

#[tokio::test]
async fn an_open_modal_dims_what_is_behind_it() {
    let path = fixture("scrim");
    let (mut app, _messages) = App::new(config(&path));

    // A cell in the header, well away from any modal.
    let (bright_fg, _) = cell_colours(&mut app, 1, 0);

    binsql::app::keys::handle(&mut app, KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
    let (dimmed_fg, _) = cell_colours(&mut app, 1, 0);

    assert_ne!(
        bright_fg, dimmed_fg,
        "the layout behind a modal should recede"
    );

    // It moved toward the background rather than to some other colour.
    let background = ratatui::style::Color::Rgb(0x1e, 0x1e, 0x2e);
    assert!(
        distance(dimmed_fg, background) < distance(bright_fg, background),
        "dimming should move a colour toward the background: \
         {bright_fg:?} -> {dimmed_fg:?}"
    );

    // The modal itself is drawn after the scrim, so it keeps its own colours.
    let screen = render(&mut app);
    let help_row = screen
        .lines()
        .position(|line| line.contains("Anywhere"))
        .expect("help drawn");
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).expect("terminal");
    terminal
        .draw(|frame| ui::draw(frame, &mut app))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    let title = (0..WIDTH)
        .map(|x| &buffer[(x, help_row as u16)])
        .find(|cell| cell.symbol() == "A")
        .expect("the Anywhere heading");
    assert_eq!(
        title.fg,
        ratatui::style::Color::Rgb(0xb4, 0xbe, 0xfe),
        "the modal should not be dimmed by its own scrim"
    );

    // Closing it puts everything back.
    press(&mut app, KeyCode::Esc);
    let (restored_fg, _) = cell_colours(&mut app, 1, 0);
    assert_eq!(restored_fg, bright_fg, "closing should restore the layout");

    let _ = std::fs::remove_file(&path);
}

fn distance(a: ratatui::style::Color, b: ratatui::style::Color) -> i32 {
    let (ratatui::style::Color::Rgb(ar, ag, ab), ratatui::style::Color::Rgb(br, bg, bb)) = (a, b)
    else {
        return i32::MAX;
    };
    (i32::from(ar) - i32::from(br)).abs()
        + (i32::from(ag) - i32::from(bg)).abs()
        + (i32::from(ab) - i32::from(bb)).abs()
}

#[tokio::test]
async fn the_splash_greets_and_any_key_dismisses_it() {
    let path = fixture("splash");
    let (mut app, _messages) = App::new(config(&path));
    app.show_splash();

    let screen = render(&mut app);
    println!("\n{screen}\n");

    assert!(screen.contains("binsql"), "wordmark missing:\n{screen}");
    assert!(
        screen.contains("a database IDE for the terminal"),
        "tagline missing:\n{screen}"
    );
    assert!(
        screen.contains(&format!("v{}", env!("CARGO_PKG_VERSION"))),
        "version missing:\n{screen}"
    );
    assert!(
        screen.contains("1 data source registered"),
        "should count what is registered, singular:\n{screen}"
    );
    assert!(
        screen.contains("Any key to begin"),
        "hint missing:\n{screen}"
    );

    press(&mut app, KeyCode::Char('x'));
    assert!(app.overlay.is_none(), "any key should dismiss the splash");
    assert!(
        !render(&mut app).contains("Any key to begin"),
        "the splash should be gone"
    );

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn the_splash_falls_back_in_a_narrow_terminal() {
    let path = fixture("splashnarrow");
    let (mut app, _messages) = App::new(config(&path));
    app.show_splash();

    // Narrower than the wordmark, which must not overflow its own box.
    let mut terminal = Terminal::new(TestBackend::new(40, 24)).expect("terminal");
    terminal
        .draw(|frame| ui::draw(frame, &mut app))
        .expect("draw");

    let rows: Vec<String> = terminal
        .backend()
        .buffer()
        .content()
        .chunks(40)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect())
        .collect();

    assert!(
        rows.iter().any(|row| row.contains("binsql")),
        "the plain wordmark should stand in:\n{}",
        rows.join("\n")
    );
    assert!(
        !rows.iter().any(|row| row.contains('\u{2588}')),
        "the block wordmark should not be drawn when it does not fit:\n{}",
        rows.join("\n")
    );

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn every_modal_hugs_its_content() {
    // Each of these used to take a fixed percentage of the screen whatever was
    // in it, so a six-field form opened a box two thirds of the terminal tall.
    let path = fixture("hug");

    let cases: [(&str, KeyEvent, &str); 3] = [
        (
            "help",
            KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE),
            "Help",
        ),
        (
            "palette",
            KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
            "Commands",
        ),
        (
            "connection form",
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
            "New data source",
        ),
    ];

    for (what, opener, title) in cases {
        let (mut app, _messages) = App::new(config(&path));
        binsql::app::keys::handle(&mut app, opener);
        let screen = render(&mut app);

        let rows: Vec<&str> = screen.lines().collect();
        let top = rows
            .iter()
            .position(|line| line.contains(title))
            .unwrap_or_else(|| panic!("{what} not drawn:\n{screen}"));
        let bottom = rows
            .iter()
            .skip(top)
            .position(|line| line.contains('╰'))
            .unwrap_or_else(|| panic!("{what} not closed:\n{screen}"))
            + top;

        // Every row inside the box carries something; none is padding to reach
        // a percentage.
        let blank_rows = rows[top + 1..bottom]
            .iter()
            .filter(|line| {
                let inside: String = line.chars().skip_while(|c| *c != '│').collect();
                inside.trim_matches(|c| c == '│' || c == ' ').is_empty()
            })
            .count();
        let inner_rows = bottom - top - 1;
        assert!(
            blank_rows * 3 <= inner_rows,
            "{what} is mostly empty: {blank_rows} blank of {inner_rows}:\n{screen}"
        );
        assert!(
            bottom - top + 1 < HEIGHT as usize,
            "{what} should not fill the screen:\n{screen}"
        );
    }

    let _ = std::fs::remove_file(&path);
}

/// A query nobody wants any more has to be stoppable from the keyboard, and the
/// console has to come back ready for the next one.
#[tokio::test]
async fn ctrl_c_cancels_a_running_query() {
    let path = fixture("cancel");
    let (mut app, mut messages) = App::new(config(&path));
    app.open_startup_sources();
    settle(&mut app, &mut messages).await;

    // Long enough that it is still running two keystrokes later.
    app.console_mut().set_sql(
        "WITH RECURSIVE counter(x) AS (\
           SELECT 1 UNION ALL SELECT x + 1 FROM counter WHERE x < 20000000\
         ) SELECT count(*) FROM counter",
    );
    binsql::app::keys::handle(
        &mut app,
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
    );

    let screen = render(&mut app);
    assert!(
        screen.contains("Running"),
        "no sign of the query:\n{screen}"
    );
    assert!(
        screen.contains("⌃C"),
        "the way out is not on screen:\n{screen}"
    );

    binsql::app::keys::handle(
        &mut app,
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
    );
    settle(&mut app, &mut messages).await;

    let screen = render(&mut app);
    assert!(
        screen.contains("Cancelled") || screen.contains("cancelled"),
        "the cancel was not reported:\n{screen}"
    );
    assert!(
        !app.console().is_running(),
        "the console is still marked as running:\n{screen}"
    );

    // And the console takes the next query as though nothing had happened.
    app.console_mut().set_sql("SELECT 1 AS after_cancel");
    binsql::app::keys::handle(
        &mut app,
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
    );
    settle(&mut app, &mut messages).await;

    let screen = render(&mut app);
    assert!(
        screen.contains("after_cancel"),
        "the console did not recover:\n{screen}"
    );

    let _ = std::fs::remove_file(&path);
}
