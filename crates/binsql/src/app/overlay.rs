//! The things that draw on top of the layout: help, one record in full, the
//! command palette, and the connection form.

use binsql_core::{Backend, DataSource};

use super::App;

pub enum Overlay {
    Help,
    /// One row, every column stacked. The grid has to truncate to fit a line;
    /// this is where the values are actually readable.
    Detail(RowDetail),
    Palette(Palette),
    Connect(ConnectForm),
}

/// Scroll position within the stacked row. A wide table is taller than the
/// modal, so this is not optional.
#[derive(Debug, Default)]
pub struct RowDetail {
    pub scroll: usize,
    /// How far down it is worth scrolling, written by the renderer once it
    /// knows how many lines the row wrapped to.
    pub max_scroll: usize,
}

impl RowDetail {
    pub fn scroll_by(&mut self, delta: isize) {
        let next = self.scroll as isize + delta;
        self.scroll = next.clamp(0, self.max_scroll as isize) as usize;
    }

    pub fn to_top(&mut self) {
        self.scroll = 0;
    }

    pub fn to_bottom(&mut self) {
        self.scroll = self.max_scroll;
    }
}

// --- command palette ---

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    NewConsole,
    CloseConsole,
    RunQuery,
    NewDataSource,
    Connect(String),
    Disconnect(String),
    BindConsole(String),
    EditDataSource(String),
    RemoveDataSource(String),
    Refresh,
    Help,
    Quit,
}

impl Command {
    pub fn label(&self) -> String {
        match self {
            Command::NewConsole => "New console".into(),
            Command::CloseConsole => "Close console".into(),
            Command::RunQuery => "Run query".into(),
            Command::NewDataSource => "New data source…".into(),
            Command::Connect(name) => format!("Connect {name}"),
            Command::Disconnect(name) => format!("Disconnect {name}"),
            Command::BindConsole(name) => format!("Run this console against {name}"),
            Command::EditDataSource(name) => format!("Edit {name}…"),
            Command::RemoveDataSource(name) => format!("Remove {name}"),
            Command::Refresh => "Refresh selected node".into(),
            Command::Help => "Help".into(),
            Command::Quit => "Quit".into(),
        }
    }

    pub fn hint(&self) -> &'static str {
        match self {
            Command::NewConsole => "⌃T",
            Command::CloseConsole => "⌃W",
            Command::RunQuery => "⌃R",
            Command::NewDataSource => "⌃N",
            Command::Refresh => "⌃F5",
            Command::Help => "?",
            Command::Quit => "⌃Q",
            _ => "",
        }
    }
}

pub struct Palette {
    pub query: String,
    pub selected: usize,
    commands: Vec<Command>,
}

impl Palette {
    /// Builds the palette from what is actually available right now — a data
    /// source that is already connected offers Disconnect, not Connect.
    pub fn build(app: &App) -> Palette {
        let mut commands = vec![
            Command::RunQuery,
            Command::NewConsole,
            Command::CloseConsole,
            Command::NewDataSource,
            Command::Refresh,
        ];

        for name in app.config.names() {
            if app.sessions.contains_key(name) {
                commands.push(Command::BindConsole(name.clone()));
                commands.push(Command::Disconnect(name.clone()));
            } else {
                commands.push(Command::Connect(name.clone()));
            }
            commands.push(Command::EditDataSource(name.clone()));
            commands.push(Command::RemoveDataSource(name.clone()));
        }

        commands.push(Command::Help);
        commands.push(Command::Quit);

        Palette {
            query: String::new(),
            selected: 0,
            commands,
        }
    }

    /// Case-insensitive subsequence match, so "ncon" finds "New console".
    pub fn matches(&self) -> Vec<&Command> {
        if self.query.is_empty() {
            return self.commands.iter().collect();
        }
        let needle = self.query.to_lowercase();
        self.commands
            .iter()
            .filter(|command| is_subsequence(&needle, &command.label().to_lowercase()))
            .collect()
    }

    pub fn selected_command(&self) -> Option<Command> {
        self.matches().get(self.selected).map(|c| (*c).clone())
    }

    pub fn move_selection(&mut self, delta: isize) {
        let count = self.matches().len();
        if count == 0 {
            self.selected = 0;
            return;
        }
        let next = (self.selected as isize + delta).rem_euclid(count as isize);
        self.selected = next as usize;
    }

    pub fn push(&mut self, ch: char) {
        self.query.push(ch);
        self.selected = 0;
    }

    pub fn backspace(&mut self) {
        self.query.pop();
        self.selected = 0;
    }
}

fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut chars = haystack.chars();
    needle
        .chars()
        .all(|wanted| chars.any(|candidate| candidate == wanted))
}

// --- connection form ---

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Name,
    Dsn,
    Backend,
    ReadOnly,
    OpenOnStart,
}

impl Field {
    pub const ORDER: [Field; 5] = [
        Field::Name,
        Field::Dsn,
        Field::Backend,
        Field::ReadOnly,
        Field::OpenOnStart,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Field::Name => "Name",
            Field::Dsn => "Connection string",
            Field::Backend => "Driver",
            Field::ReadOnly => "Read-only",
            Field::OpenOnStart => "Open at startup",
        }
    }
}

pub struct ConnectForm {
    pub name: String,
    pub dsn: String,
    /// `None` means "work it out from the connection string", which is right
    /// almost always and saves a decision.
    pub backend: Option<Backend>,
    pub read_only: bool,
    pub open_on_start: bool,
    pub field: Field,
    pub error: Option<String>,
    /// Set when editing, so a rename can remove the old entry.
    pub editing: Option<String>,
}

impl ConnectForm {
    pub fn new() -> ConnectForm {
        ConnectForm {
            name: String::new(),
            dsn: String::new(),
            backend: None,
            read_only: false,
            open_on_start: false,
            field: Field::Name,
            error: None,
            editing: None,
        }
    }

    pub fn editing(name: &str, source: &DataSource) -> ConnectForm {
        ConnectForm {
            name: name.to_string(),
            dsn: source.dsn.clone(),
            backend: Some(source.backend),
            read_only: source.read_only,
            open_on_start: source.open_on_start,
            field: Field::Name,
            error: None,
            editing: Some(name.to_string()),
        }
    }

    /// The driver as it will be used: the explicit choice, or what the
    /// connection string implies.
    pub fn effective_backend(&self) -> Option<Backend> {
        self.backend.or_else(|| Backend::infer(&self.dsn))
    }

    pub fn backend_display(&self) -> String {
        match self.backend {
            Some(backend) => backend.label().to_string(),
            None => match Backend::infer(&self.dsn) {
                Some(backend) => format!("auto — {}", backend.label()),
                None => "auto".to_string(),
            },
        }
    }

    pub fn next_field(&mut self, delta: isize) {
        let position = Field::ORDER
            .iter()
            .position(|f| *f == self.field)
            .unwrap_or(0) as isize;
        let next = (position + delta).rem_euclid(Field::ORDER.len() as isize);
        self.field = Field::ORDER[next as usize];
    }

    /// Cycles the driver, including back through "auto".
    pub fn cycle_backend(&mut self, delta: isize) {
        let all = Backend::ALL;
        self.backend = match self.backend {
            None if delta > 0 => Some(all[0]),
            None => Some(all[all.len() - 1]),
            Some(current) => {
                let position = all.iter().position(|b| *b == current).unwrap_or(0) as isize;
                let next = position + delta;
                if next < 0 || next >= all.len() as isize {
                    None
                } else {
                    Some(all[next as usize])
                }
            }
        };
    }

    pub fn toggle(&mut self) {
        match self.field {
            Field::ReadOnly => self.read_only = !self.read_only,
            Field::OpenOnStart => self.open_on_start = !self.open_on_start,
            _ => {}
        }
    }

    pub fn push(&mut self, ch: char) {
        match self.field {
            Field::Name => self.name.push(ch),
            Field::Dsn => self.dsn.push(ch),
            Field::ReadOnly | Field::OpenOnStart if ch == ' ' => self.toggle(),
            _ => {}
        }
    }

    pub fn backspace(&mut self) {
        match self.field {
            Field::Name => {
                self.name.pop();
            }
            Field::Dsn => {
                self.dsn.pop();
            }
            _ => {}
        }
    }

    /// Validates and returns what to save.
    pub fn build(&self) -> Result<(String, DataSource), String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err("A data source needs a name".into());
        }
        let dsn = self.dsn.trim();
        if dsn.is_empty() {
            return Err("A data source needs a connection string".into());
        }
        let backend = self.effective_backend().ok_or_else(|| {
            "Could not tell the driver from that connection string — pick one".to_string()
        })?;

        Ok((
            name.to_string(),
            DataSource {
                backend,
                dsn: dsn.to_string(),
                description: String::new(),
                read_only: self.read_only,
                open_on_start: self.open_on_start,
            },
        ))
    }
}

impl Default for ConnectForm {
    fn default() -> Self {
        ConnectForm::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subsequence_matching_finds_abbreviations() {
        assert!(is_subsequence("ncon", "new console"));
        assert!(is_subsequence("quit", "quit"));
        assert!(!is_subsequence("zzz", "new console"));
    }

    #[test]
    fn backend_cycles_through_auto() {
        let mut form = ConnectForm::new();
        assert_eq!(form.backend, None);
        form.cycle_backend(1);
        assert_eq!(form.backend, Some(Backend::Sqlite));
        form.cycle_backend(-1);
        assert_eq!(form.backend, None);
    }

    #[test]
    fn form_infers_the_driver_from_the_dsn() {
        let mut form = ConnectForm::new();
        form.name = "local".into();
        form.dsn = "postgres://localhost/app".into();
        let (name, source) = form.build().expect("valid form");
        assert_eq!(name, "local");
        assert_eq!(source.backend, Backend::Postgres);
    }

    #[test]
    fn form_rejects_an_unrecognisable_dsn() {
        let mut form = ConnectForm::new();
        form.name = "local".into();
        form.dsn = "something odd".into();
        assert!(form.build().is_err());
    }
}
