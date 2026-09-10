//! The things that draw on top of the layout: help, one record in full, the
//! command palette, and the connection form.

use binsql_core::secrets::keychain;
use binsql_core::{Backend, DataSource};

use super::App;

pub enum Overlay {
    /// Shown once at startup. Any key dismisses it.
    Splash,
    Help,
    /// One row, every column stacked. The grid has to truncate to fit a line;
    /// this is where the values are actually readable.
    Detail(RowDetail),
    /// One field of that row, in full. A JSON document or a paragraph of text
    /// is still cramped in the record's value column.
    Value(ValueDetail),
    Palette(Palette),
    Connect(ConnectForm),
}

/// Where the record view is scrolled to.
///
/// Which field is selected is not kept here — it is the grid's cursor column,
/// so stepping through fields in the modal moves the grid with it and closing
/// leaves you on the field you were reading.
#[derive(Debug, Default)]
pub struct RowDetail {
    /// First visible line. Follows the selection at render time, and is kept
    /// only so the view moves the least it can when the selection walks off an
    /// edge.
    pub scroll: usize,
}

/// One field, in full, with its own scroll.
#[derive(Debug, Default)]
pub struct ValueDetail {
    /// The record view this was opened from, restored when it closes — the
    /// value modal is a step into the record, not a replacement for it.
    pub row: RowDetail,
    pub scroll: usize,
    /// How far down it is worth scrolling, written by the renderer once it
    /// knows how many lines the value wrapped to.
    pub max_scroll: usize,
}

impl ValueDetail {
    pub fn new(row: RowDetail) -> ValueDetail {
        ValueDetail {
            row,
            scroll: 0,
            max_scroll: 0,
        }
    }

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
    CancelQuery,
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
            Command::CancelQuery => "Cancel the running query".into(),
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
            Command::CancelQuery => "⌃C",
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

        // Only worth offering while there is something to call off.
        if app.console().is_running() {
            commands.insert(1, Command::CancelQuery);
        }

        for (name, _) in app.config.iter() {
            if app.sessions.contains_key(&name) {
                commands.push(Command::BindConsole(name.clone()));
                commands.push(Command::Disconnect(name.clone()));
            } else {
                commands.push(Command::Connect(name.clone()));
            }
            commands.push(Command::EditDataSource(name.clone()));
            commands.push(Command::RemoveDataSource(name));
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
    Folder,
    Name,
    Dsn,
    Keychain,
    Backend,
    ReadOnly,
    OpenOnStart,
}

impl Field {
    pub const ORDER: [Field; 7] = [
        Field::Folder,
        Field::Name,
        Field::Dsn,
        Field::Keychain,
        Field::Backend,
        Field::ReadOnly,
        Field::OpenOnStart,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Field::Folder => "Folder",
            Field::Name => "Name",
            Field::Dsn => "Connection string",
            Field::Keychain => "Stored in",
            Field::Backend => "Driver",
            Field::ReadOnly => "Read-only",
            Field::OpenOnStart => "Open at startup",
        }
    }
}

/// What the form produced: where to save it, and — when the connection string
/// is to live in the credential store rather than the config — the string to
/// file there first.
pub struct Saved {
    pub id: String,
    pub source: DataSource,
    /// The connection string to write to the credential store under `id`.
    /// `None` leaves the store alone, either because the string is in the
    /// config or because an edit did not retype it.
    pub secret: Option<String>,
    /// The name this was saved under before, so a rename can take the old entry
    /// with it.
    pub previous: Option<String>,
}

pub struct ConnectForm {
    /// Groups the connection in the sidebar. Empty leaves it at the top level.
    pub folder: String,
    pub name: String,
    pub dsn: String,
    /// Keep the connection string in the OS credential store, leaving only a
    /// `keychain://` reference in the config.
    pub keychain: bool,
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
            folder: String::new(),
            name: String::new(),
            dsn: String::new(),
            // The safe default. A connection string typed into a new data
            // source is nearly always one with a password in it, and the config
            // is the wrong place for that.
            keychain: true,
            backend: None,
            read_only: false,
            open_on_start: false,
            field: Field::Folder,
            error: None,
            editing: None,
        }
    }

    pub fn editing(id: &str, source: &DataSource) -> ConnectForm {
        let (folder, name) = binsql_core::config::split_qualified(id);
        ConnectForm {
            folder: folder.unwrap_or_default().to_string(),
            name: name.to_string(),
            // A stored string is shown as the reference it is, not fetched.
            // Opening the store to fill in a field would put a system
            // permission prompt in front of an edit that is usually only
            // renaming something or flipping read-only.
            dsn: source.dsn.clone(),
            keychain: keychain::is_reference(&source.dsn),
            backend: Some(source.backend),
            read_only: source.read_only,
            open_on_start: source.open_on_start,
            field: Field::Name,
            error: None,
            editing: Some(id.to_string()),
        }
    }

    /// Where the connection string will end up, for the form to show.
    pub fn storage_display(&self) -> &'static str {
        if self.keychain {
            keychain::STORE_NAME
        } else {
            "the config file"
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
            Field::Keychain => self.keychain = !self.keychain,
            Field::ReadOnly => self.read_only = !self.read_only,
            Field::OpenOnStart => self.open_on_start = !self.open_on_start,
            _ => {}
        }
    }

    pub fn push(&mut self, ch: char) {
        match self.field {
            Field::Folder => self.folder.push(ch),
            Field::Name => self.name.push(ch),
            Field::Dsn => self.dsn.push(ch),
            Field::Keychain | Field::ReadOnly | Field::OpenOnStart if ch == ' ' => self.toggle(),
            _ => {}
        }
    }

    pub fn backspace(&mut self) {
        match self.field {
            Field::Folder => {
                self.folder.pop();
            }
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
    pub fn build(&self) -> Result<Saved, String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err("A data source needs a name".into());
        }
        let folder = self.folder.trim();
        if name.contains(binsql_core::config::SEPARATOR)
            || folder.contains(binsql_core::config::SEPARATOR)
        {
            return Err(format!(
                "'{}' separates the folder from the name, so neither may contain it",
                binsql_core::config::SEPARATOR
            ));
        }
        let dsn = self.dsn.trim();
        if dsn.is_empty() {
            return Err("A data source needs a connection string".into());
        }
        let backend = self.effective_backend().ok_or_else(|| {
            "Could not tell the driver from that connection string — pick one".to_string()
        })?;

        let id = binsql_core::config::qualify(Some(folder).filter(|f| !f.is_empty()), name);

        // Three cases. An untouched reference is re-keyed to the name being
        // saved under and the store is moved, not rewritten. A typed string
        // with the toggle on becomes a reference and the string is filed. With
        // the toggle off it stays in the config as it always did.
        let (stored_dsn, secret) = if keychain::is_reference(dsn) {
            if !self.keychain {
                // Moving it back into the config would mean reading the store
                // to find what to write, which is not something a form should
                // do behind a toggle. Retyping the string says it deliberately.
                return Err(format!(
                    "To move this out of the {}, clear the connection string and type it again",
                    keychain::STORE_NAME
                ));
            }
            (keychain::reference(&id), None)
        } else if self.keychain {
            (keychain::reference(&id), Some(dsn.to_string()))
        } else {
            (dsn.to_string(), None)
        };

        Ok(Saved {
            source: DataSource {
                backend,
                dsn: stored_dsn,
                description: String::new(),
                read_only: self.read_only,
                open_on_start: self.open_on_start,
            },
            id,
            secret,
            previous: self.editing.clone(),
        })
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

    /// The form defaults to storing the string, so a test that cares about the
    /// config's contents has to say so.
    fn in_the_config(name: &str, dsn: &str) -> ConnectForm {
        let mut form = ConnectForm::new();
        form.name = name.into();
        form.dsn = dsn.into();
        form.keychain = false;
        form
    }

    #[test]
    fn form_infers_the_driver_from_the_dsn() {
        let saved = in_the_config("local", "postgres://localhost/app")
            .build()
            .expect("valid form");
        assert_eq!(saved.id, "local");
        assert_eq!(saved.source.backend, Backend::Postgres);
        assert_eq!(saved.source.dsn, "postgres://localhost/app");
        assert_eq!(saved.secret, None);
    }

    #[test]
    fn a_folder_qualifies_the_saved_name() {
        let mut form = in_the_config("prod", "sqlserver://host/db");
        form.folder = "eimskip".into();
        assert_eq!(form.build().expect("valid form").id, "eimskip/prod");
    }

    #[test]
    fn the_keychain_keeps_the_string_and_the_config_keeps_a_reference() {
        let mut form = ConnectForm::new();
        form.folder = "eimskip".into();
        form.name = "prod".into();
        form.dsn = "sqlserver://sa:hunter2@host/db".into();

        let saved = form.build().expect("valid form");
        assert_eq!(saved.source.dsn, "keychain://eimskip/prod");
        assert_eq!(
            saved.secret.as_deref(),
            Some("sqlserver://sa:hunter2@host/db")
        );
        // The driver still comes off the real string, not the reference.
        assert_eq!(saved.source.backend, Backend::MsSql);
    }

    #[test]
    fn editing_a_stored_source_shows_the_reference_and_files_nothing_new() {
        let source = DataSource {
            backend: Backend::MsSql,
            dsn: "keychain://eimskip/prod".into(),
            description: String::new(),
            read_only: true,
            open_on_start: false,
        };
        let mut form = ConnectForm::editing("eimskip/prod", &source);
        assert!(form.keychain);
        assert_eq!(form.dsn, "keychain://eimskip/prod");

        // A rename re-keys the reference; the secret moves rather than being
        // rewritten, so there is nothing to file.
        form.name = "production".into();
        let saved = form.build().expect("valid form");
        assert_eq!(saved.source.dsn, "keychain://eimskip/production");
        assert_eq!(saved.secret, None);
        assert_eq!(saved.previous.as_deref(), Some("eimskip/prod"));
    }

    #[test]
    fn a_stored_source_is_not_silently_moved_back_into_the_config() {
        let source = DataSource {
            backend: Backend::MsSql,
            dsn: "keychain://eimskip/prod".into(),
            description: String::new(),
            read_only: false,
            open_on_start: false,
        };
        let mut form = ConnectForm::editing("eimskip/prod", &source);
        form.keychain = false;
        assert!(form.build().is_err());
    }

    #[test]
    fn the_separator_is_refused_inside_a_name() {
        let mut form = ConnectForm::new();
        form.name = "eimskip/prod".into();
        form.dsn = "sqlserver://host/db".into();
        assert!(form.build().is_err());
    }

    #[test]
    fn form_rejects_an_unrecognisable_dsn() {
        let mut form = ConnectForm::new();
        form.name = "local".into();
        form.dsn = "something odd".into();
        assert!(form.build().is_err());
    }
}
