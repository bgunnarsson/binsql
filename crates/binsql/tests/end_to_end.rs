//! Drives the real application against a real database and renders it to a
//! test backend. This is the only way to assert that the thing people actually
//! look at is correct; unit tests on the model would pass with a blank screen.

use std::time::Duration;

use binsql::app::{App, Message, Pane};
use binsql::ui;
use binsql_core::{Backend, Config, DataSource};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
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
