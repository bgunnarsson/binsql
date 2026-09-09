//! binsql — a database IDE for the terminal.

use anyhow::{Context, Result, bail};
use binsql_core::{Backend, Config, DataSource};
use crossterm::event::{Event, EventStream};
use futures_util::StreamExt;

use binsql::app::{self, App, keys};
use binsql::ui;

const HELP: &str = "\
binsql — a database IDE for the terminal

USAGE
    binsql                     open the saved data sources
    binsql <connection>        open a saved data source by name, or a DSN
    binsql --driver <name> <dsn>

OPTIONS
    -d, --driver <name>   sqlite | postgres | mssql | mysql (default: inferred)
    -h, --help            show this
    -V, --version         show the version

Data sources are stored in ~/.config/binsql/connections.json and can be added
from inside the app with ⌃N.
";

#[tokio::main]
async fn main() -> Result<()> {
    let Some(options) = parse_args(std::env::args().skip(1).collect())? else {
        return Ok(());
    };

    let mut config = Config::load().context("loading connections")?;
    let mut open_now: Option<String> = None;

    if let Some(target) = options.target {
        let name = match config.get(&target) {
            // A name that is already saved wins over treating it as a DSN.
            Some(_) => target,
            None => {
                let backend = options
                    .backend
                    .or_else(|| Backend::infer(&target))
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "Could not tell which driver {target} needs — pass --driver"
                        )
                    })?;
                // Not written to disk: an ad-hoc DSN is for this run only.
                config.set(
                    "ad-hoc",
                    DataSource {
                        backend,
                        dsn: target,
                        description: String::new(),
                        read_only: false,
                        open_on_start: true,
                    },
                );
                "ad-hoc".to_string()
            }
        };
        open_now = Some(name);
    }

    let (mut app, mut messages) = App::new(config);
    match open_now {
        Some(name) => app.bind_console(&name),
        None => app.open_startup_sources(),
    }

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut app, &mut messages).await;
    ratatui::restore();
    result
}

async fn run(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    messages: &mut tokio::sync::mpsc::UnboundedReceiver<app::Message>,
) -> Result<()> {
    let mut events = EventStream::new();

    loop {
        terminal.draw(|frame| ui::draw(frame, app))?;

        tokio::select! {
            event = events.next() => match event {
                Some(Ok(Event::Key(key))) => keys::handle(app, key),
                Some(Ok(Event::Resize(_, _))) => {}
                Some(Ok(_)) => {}
                Some(Err(error)) => return Err(error.into()),
                // stdin closed; there is no way to drive the UI any more.
                None => break,
            },
            message = messages.recv() => match message {
                Some(message) => app.handle(message),
                None => break,
            },
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

struct Options {
    target: Option<String>,
    backend: Option<Backend>,
}

/// Returns `None` when the argument itself was the whole job — `--help` and
/// `--version` print and stop.
fn parse_args(args: Vec<String>) -> Result<Option<Options>> {
    let mut target = None;
    let mut backend = None;
    let mut rest = args.into_iter();

    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return Ok(None);
            }
            "-V" | "--version" => {
                println!("binsql {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "-d" | "--driver" => {
                let name = rest
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--driver needs a name"))?;
                backend = Some(Backend::parse(&name)?);
            }
            other if other.starts_with('-') => bail!("Unknown option {other}. Try --help."),
            other if target.is_none() => target = Some(other.to_string()),
            other => bail!("Unexpected argument {other}. Try --help."),
        }
    }

    Ok(Some(Options { target, backend }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_a_bare_target() {
        let options = parse_args(args(&["local"])).unwrap().unwrap();
        assert_eq!(options.target.as_deref(), Some("local"));
        assert_eq!(options.backend, None);
    }

    #[test]
    fn parses_an_explicit_driver() {
        let options = parse_args(args(&["--driver", "mysql", "/tmp/x"]))
            .unwrap()
            .unwrap();
        assert_eq!(options.backend, Some(Backend::MySql));
        assert_eq!(options.target.as_deref(), Some("/tmp/x"));
    }

    #[test]
    fn help_stops_before_starting() {
        assert!(parse_args(args(&["--help"])).unwrap().is_none());
    }

    #[test]
    fn rejects_unknown_options() {
        assert!(parse_args(args(&["--nope"])).is_err());
    }
}
