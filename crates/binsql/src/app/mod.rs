pub mod console;
pub mod keys;
pub mod mouse;
pub mod overlay;
pub mod tree;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ratatui::layout::Rect;

use binsql_core::secrets::keychain;
use binsql_core::{Catalog, Column, Error, ObjectKind, ObjectRef, ResultSet, Session, Workspace};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio_util::sync::CancellationToken;

use console::{Console, DEFAULT_LIMIT, Grid, Outcome};
use overlay::{Overlay, Saved};
use tree::{LoadState, Node, NodeId, NodeKind, Tree};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Explorer,
    Editor,
    Results,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Info,
    Success,
    Warning,
    Error,
}

pub struct Status {
    pub text: String,
    pub tone: Tone,
    pub at: Instant,
}

impl Status {
    fn new(text: impl Into<String>, tone: Tone) -> Status {
        Status {
            text: text.into(),
            tone,
            at: Instant::now(),
        }
    }

    /// Transient messages fade so the bar goes back to showing context.
    pub fn is_stale(&self) -> bool {
        self.tone == Tone::Info && self.at.elapsed() > Duration::from_secs(6)
    }
}

/// Work finished off the UI thread. Every variant names what it belongs to, so
/// a reply that arrives after the user has moved on is dropped rather than
/// applied to the wrong place.
pub enum Message {
    Connected {
        node: NodeId,
        name: String,
        session: Arc<Session>,
    },
    ConnectFailed {
        node: NodeId,
        name: String,
        error: String,
    },
    Catalogs {
        node: NodeId,
        catalogs: Vec<Catalog>,
    },
    Schemas {
        node: NodeId,
        schemas: Vec<String>,
    },
    Objects {
        node: NodeId,
        objects: Vec<ObjectRef>,
    },
    Columns {
        node: NodeId,
        columns: Vec<Column>,
    },
    LoadFailed {
        node: NodeId,
        error: String,
    },
    QueryFinished {
        console: u64,
        generation: u64,
        /// The core's own error, not a string: a cancelled query is a
        /// different thing on screen from a failed one.
        result: Result<ResultSet, Error>,
    },
}

pub struct App {
    /// Reads like a `Config`, because it derefs to the merged one. Writes name
    /// the file they mean.
    pub config: Workspace,
    pub sessions: HashMap<String, Arc<Session>>,
    pub tree: Tree,
    pub consoles: Vec<Console>,
    pub active_console: usize,
    pub focus: Pane,
    pub overlay: Option<Overlay>,
    pub status: Status,
    pub should_quit: bool,
    /// The sidebar width and query-pane height someone has dragged to, if they
    /// have. Held raw and clamped at draw time, where the terminal's size is
    /// known — so a window resize re-fits them instead of stranding them.
    pub explorer_width: Option<u16>,
    pub editor_height: Option<u16>,
    /// What the mouse is in the middle of doing, while it is doing it.
    pub dragging: Option<Drag>,
    /// The last press — column, row, when — for telling a double click from
    /// two clicks. The terminal reports presses and leaves the pairing to
    /// whoever wants it.
    pub last_press: Option<(u16, u16, Instant)>,
    pub panes: PaneAreas,
    /// Where each tab was drawn along the strip. Written by the renderer: the
    /// tabs are as wide as their titles, so where one ends is only known by
    /// laying them out.
    pub tabs: Vec<TabSpan>,
    next_console_id: u64,
    tx: UnboundedSender<Message>,
}

/// What a held mouse button is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drag {
    /// The seam between the sidebar and the workspace. Moves left and right.
    Sidebar,
    /// The seam between the query pane and the results. Moves up and down.
    Results,
    /// Selecting text in the query editor. Kept for the whole gesture so a
    /// selection carries on when the pointer wanders out of the pane.
    Text,
}

/// Where the panes were last drawn.
///
/// Written by the renderer, which is the only thing that knows the layout, and
/// read by the mouse handler, which has nothing to go on but a column and a
/// row. The bordered panes locate the seams between them; the areas inside
/// those borders are what a click has to be measured against.
#[derive(Debug, Clone, Copy, Default)]
pub struct PaneAreas {
    pub explorer: Rect,
    pub editor: Rect,
    pub results: Rect,
    pub tree: Rect,
    pub text: Rect,
    pub grid: Rect,
    pub tabs: Rect,
}

/// One tab's place along the strip, and what clicking it does.
#[derive(Debug, Clone, Copy)]
pub struct TabSpan {
    /// The console this tab opens, or `None` for the `+` that makes a new one.
    pub console: Option<usize>,
    pub start: u16,
    pub end: u16,
}

impl App {
    pub fn new(config: Workspace) -> (App, UnboundedReceiver<Message>) {
        let (tx, rx) = unbounded_channel();

        let mut tree = Tree::new();
        for (id, _) in config.iter() {
            tree.add_source(id);
        }

        let mut app = App {
            config,
            sessions: HashMap::new(),
            tree,
            consoles: Vec::new(),
            active_console: 0,
            focus: Pane::Explorer,
            overlay: None,
            status: Status::new("⌃K for commands, ? for help", Tone::Info),
            should_quit: false,
            explorer_width: None,
            editor_height: None,
            dragging: None,
            last_press: None,
            panes: PaneAreas::default(),
            tabs: Vec::new(),
            next_console_id: 0,
            tx,
        };
        app.new_console();
        // `new_console` focuses the editor, which is right when someone presses
        // ⌃T and wrong at startup: the first thing anyone does is pick a
        // database.
        app.focus = Pane::Explorer;

        (app, rx)
    }

    /// Shows the greeting. Called when the app launches rather than when its
    /// state is built, so a test drives the layout without dismissing a splash
    /// first.
    pub fn show_splash(&mut self) {
        self.overlay = Some(Overlay::Splash);
    }

    /// Connects the data sources marked to open at startup.
    pub fn open_startup_sources(&mut self) {
        let names = self.config.startup_sources();

        // A `default` that names two things opens neither, which on its own
        // looks exactly like having no default at all.
        if names.is_empty()
            && let Some(problem) = self.config.unresolved_default()
        {
            self.warn(problem);
        }

        for name in names {
            if let Some(node) = self.source_node_id(&name) {
                self.connect(node, &name);
            }
        }
    }

    // --- status ---

    pub fn info(&mut self, text: impl Into<String>) {
        self.status = Status::new(text, Tone::Info);
    }

    pub fn success(&mut self, text: impl Into<String>) {
        self.status = Status::new(text, Tone::Success);
    }

    pub fn warn(&mut self, text: impl Into<String>) {
        self.status = Status::new(text, Tone::Warning);
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.status = Status::new(text, Tone::Error);
    }

    // --- consoles ---

    pub fn console(&self) -> &Console {
        &self.consoles[self.active_console]
    }

    pub fn console_mut(&mut self) -> &mut Console {
        &mut self.consoles[self.active_console]
    }

    pub fn new_console(&mut self) {
        self.next_console_id += 1;
        let id = self.next_console_id;
        let mut console = Console::new(format!("query {id}"));
        console.id = id;

        // A new tab inherits the current binding — opening one to run another
        // statement against the same database is the common case.
        if let Some(current) = self.consoles.get(self.active_console) {
            console.source = current.source.clone();
            console.catalog = current.catalog.clone();
        }

        self.consoles.push(console);
        self.active_console = self.consoles.len() - 1;
        self.focus = Pane::Editor;
    }

    pub fn close_console(&mut self) {
        if self.consoles.len() <= 1 {
            self.warn("The last console stays open");
            return;
        }
        self.consoles.remove(self.active_console);
        self.active_console = self.active_console.min(self.consoles.len() - 1);
    }

    pub fn select_console(&mut self, index: usize) {
        if index < self.consoles.len() {
            self.active_console = index;
        }
    }

    pub fn cycle_console(&mut self, delta: isize) {
        let count = self.consoles.len() as isize;
        if count == 0 {
            return;
        }
        let next = (self.active_console as isize + delta).rem_euclid(count);
        self.active_console = next as usize;
    }

    // --- tree ---

    fn source_node_id(&self, name: &str) -> Option<NodeId> {
        self.tree.source_node(name).map(|node| node.id)
    }

    /// Enter on a node: connect, drill in, or open the table.
    /// Whether this press pairs with the one before it into a double click.
    ///
    /// The row has to match and the column has to be close: two panes side by
    /// side share their rows, so the column is what keeps a click in the tree
    /// from pairing with one in the grid. The record is consumed either way,
    /// so a third press starts a new pair rather than reading as a second
    /// double.
    pub fn double_click(&mut self, column: u16, row: u16) -> bool {
        /// How close together two presses have to land to be one gesture.
        const WITHIN: Duration = Duration::from_millis(400);
        /// How far the pointer may drift between them, in columns.
        const DRIFT: u16 = 2;

        let now = Instant::now();
        let paired = self.last_press.is_some_and(|(before, at, when)| {
            row == at && column.abs_diff(before) <= DRIFT && now.duration_since(when) <= WITHIN
        });
        self.last_press = if paired {
            None
        } else {
            Some((column, row, now))
        };
        paired
    }

    pub fn activate_selected(&mut self) {
        let Some(id) = self.tree.selected_id() else {
            return;
        };
        let Some(node) = self.tree.find(id) else {
            return;
        };

        match &node.kind {
            NodeKind::Object { object } => {
                let object = object.clone();
                self.open_object(id, object);
            }
            NodeKind::ColumnNode { .. } | NodeKind::Note { .. } => {}
            _ => self.toggle_selected(),
        }
    }

    /// Space / l on a node: expand it, loading its children if needed.
    pub fn toggle_selected(&mut self) {
        let Some(id) = self.tree.selected_id() else {
            return;
        };
        let Some(node) = self.tree.find(id) else {
            return;
        };
        if !node.is_expandable() {
            return;
        }

        if node.expanded {
            if let Some(node) = self.tree.find_mut(id) {
                node.expanded = false;
            }
            return;
        }

        match node.state {
            LoadState::Loaded => {
                if let Some(node) = self.tree.find_mut(id) {
                    node.expanded = true;
                }
            }
            LoadState::Loading => {}
            LoadState::Pending | LoadState::Failed(_) => self.load_children(id),
            LoadState::Leaf => {}
        }
    }

    /// Reloads a node from the server, discarding what was cached.
    pub fn refresh_selected(&mut self) {
        let Some(id) = self.tree.selected_id() else {
            return;
        };
        self.tree.invalidate(id);
        self.load_children(id);
    }

    fn load_children(&mut self, id: NodeId) {
        let Some(node) = self.tree.find(id) else {
            return;
        };
        let kind = node.kind.clone();
        let Some(context) = self.tree.context(id) else {
            return;
        };

        match kind {
            NodeKind::Source { name, connected } => {
                if connected {
                    self.load_catalogs(id, &name);
                } else {
                    self.connect(id, &name);
                }
            }
            NodeKind::Catalog { name, .. } => {
                let Some(session) = context.source.and_then(|s| self.sessions.get(&s)).cloned()
                else {
                    return;
                };
                if session.backend().has_schemas() {
                    self.load_schemas(id, session, name);
                } else {
                    self.load_objects(id, session, name, None);
                }
            }
            NodeKind::Schema { name } => {
                let (Some(source), Some(catalog)) = (context.source, context.catalog) else {
                    return;
                };
                let Some(session) = self.sessions.get(&source).cloned() else {
                    return;
                };
                self.load_objects(id, session, catalog, Some(name));
            }
            NodeKind::Folder { .. } => {
                // A folder holds config entries, not database objects; there
                // is nothing behind it to fetch.
                if let Some(node) = self.tree.find_mut(id) {
                    node.expanded = true;
                    node.state = LoadState::Loaded;
                }
            }
            NodeKind::Group { .. } => {
                // Groups are filled when their schema loads; expanding one is
                // never a round trip.
                if let Some(node) = self.tree.find_mut(id) {
                    node.expanded = true;
                    node.state = LoadState::Loaded;
                }
            }
            NodeKind::Object { object } => {
                let Some(session) = context.source.and_then(|s| self.sessions.get(&s)).cloned()
                else {
                    return;
                };
                self.load_columns(id, session, object);
            }
            NodeKind::ColumnNode { .. } | NodeKind::Note { .. } => {}
        }
    }

    fn connect(&mut self, node: NodeId, name: &str) {
        let Some(source) = self.config.get(name).cloned() else {
            self.error(format!("No data source named {name}"));
            return;
        };
        self.tree.set_loading(node);
        self.info(format!("Connecting to {name}…"));

        let tx = self.tx.clone();
        let name = name.to_string();
        tokio::spawn(async move {
            let message = match Session::open(name.clone(), source).await {
                Ok(session) => Message::Connected {
                    node,
                    name,
                    session: Arc::new(session),
                },
                Err(error) => Message::ConnectFailed {
                    node,
                    name,
                    error: error.to_string(),
                },
            };
            let _ = tx.send(message);
        });
    }

    pub fn disconnect(&mut self, name: &str) {
        self.sessions.remove(name);
        if let Some(id) = self.source_node_id(name) {
            self.tree.invalidate(id);
            self.tree.mark_connected(id, false);
        }
        for console in &mut self.consoles {
            if console.source.as_deref() == Some(name) {
                console.source = None;
                console.catalog = None;
            }
        }
        self.info(format!("Disconnected {name}"));
    }

    fn load_catalogs(&mut self, node: NodeId, name: &str) {
        let Some(session) = self.sessions.get(name).cloned() else {
            return;
        };
        self.tree.set_loading(node);
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let message = match session.catalogs().await {
                Ok(catalogs) => Message::Catalogs { node, catalogs },
                Err(error) => Message::LoadFailed {
                    node,
                    error: error.to_string(),
                },
            };
            let _ = tx.send(message);
        });
    }

    fn load_schemas(&mut self, node: NodeId, session: Arc<Session>, catalog: String) {
        self.tree.set_loading(node);
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let message = match session.schemas(&catalog).await {
                Ok(schemas) => Message::Schemas { node, schemas },
                Err(error) => Message::LoadFailed {
                    node,
                    error: error.to_string(),
                },
            };
            let _ = tx.send(message);
        });
    }

    fn load_objects(
        &mut self,
        node: NodeId,
        session: Arc<Session>,
        catalog: String,
        schema: Option<String>,
    ) {
        self.tree.set_loading(node);
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let message = match session.objects(&catalog, schema.as_deref()).await {
                Ok(objects) => Message::Objects { node, objects },
                Err(error) => Message::LoadFailed {
                    node,
                    error: error.to_string(),
                },
            };
            let _ = tx.send(message);
        });
    }

    fn load_columns(&mut self, node: NodeId, session: Arc<Session>, object: ObjectRef) {
        self.tree.set_loading(node);
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let message = match session.columns(&object).await {
                Ok(columns) => Message::Columns { node, columns },
                Err(error) => Message::LoadFailed {
                    node,
                    error: error.to_string(),
                },
            };
            let _ = tx.send(message);
        });
    }

    // --- queries ---

    /// Opens a table in the active console and runs the preview.
    fn open_object(&mut self, node: NodeId, object: ObjectRef) {
        let Some(context) = self.tree.context(node) else {
            return;
        };
        let (Some(source), Some(catalog)) = (context.source, context.catalog.clone()) else {
            return;
        };
        let Some(session) = self.sessions.get(&source).cloned() else {
            return;
        };

        let dialect = session.dialect();
        let sql = dialect.select_limit(&object.qualified(&dialect), DEFAULT_LIMIT);

        // A console holding unsaved work is not reused: the table gets a new
        // tab, so nothing anyone typed is thrown away.
        if !self.console().sql().trim().is_empty() && self.console().grid().is_none() {
            self.new_console();
        }

        let console = self.console_mut();
        console.source = Some(source);
        console.catalog = Some(catalog);
        console.title = object.display();
        console.set_sql(&sql);

        self.run_query();
    }

    pub fn run_query(&mut self) {
        let index = self.active_console;
        let sql = self.consoles[index].sql_to_run();
        if sql.trim().is_empty() {
            self.warn("Nothing to run");
            return;
        }

        let Some(name) = self.consoles[index].source.clone() else {
            self.warn("This console is not bound to a data source — pick one with ⌃K");
            return;
        };
        let Some(session) = self.sessions.get(&name).cloned() else {
            self.error(format!("{name} is not connected"));
            return;
        };

        let console = &mut self.consoles[index];
        // A console runs one query at a time, so starting another calls off
        // whatever the last one left running rather than racing it.
        console.cancel_query();
        console.generation += 1;
        let generation = console.generation;
        let id = console.id;
        let catalog = console.catalog.clone();
        let cancel = CancellationToken::new();
        console.cancel = Some(cancel.clone());
        console.outcome = Outcome::Running;

        self.focus = Pane::Results;
        self.info("Running… ⌃C cancels");

        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result = session
                .run_cancellable(catalog.as_deref(), &sql, Some(DEFAULT_LIMIT), &cancel)
                .await;
            let _ = tx.send(Message::QueryFinished {
                console: id,
                generation,
                result,
            });
        });
    }

    /// Calls off the query the active console is running.
    ///
    /// The console is free again as soon as this returns; what it costs the
    /// server is the backend's business, and every one of them here is told to
    /// stop in whatever way it understands.
    pub fn cancel_query(&mut self) {
        if self.console_mut().cancel_query() {
            self.info("Cancelling…");
        }
    }

    /// Points the active console at a data source, connecting if needed.
    pub fn bind_console(&mut self, name: &str) {
        let catalog = self
            .sessions
            .get(name)
            .and_then(|session| session.current_catalog().map(str::to_string));
        let console = self.console_mut();
        console.source = Some(name.to_string());
        console.catalog = catalog;

        if !self.sessions.contains_key(name)
            && let Some(node) = self.source_node_id(name)
        {
            self.connect(node, name);
            return;
        }
        self.success(format!("Console bound to {name}"));
    }

    // --- messages ---

    pub fn handle(&mut self, message: Message) {
        match message {
            Message::Connected {
                node,
                name,
                session,
            } => {
                let version = session.server_version().unwrap_or_default().to_string();
                self.sessions.insert(name.clone(), session);
                self.tree.mark_connected(node, true);
                self.load_catalogs(node, &name);

                // The first connection adopts any console that has no binding,
                // so opening binsql and typing a query just works.
                for console in &mut self.consoles {
                    if console.source.is_none() {
                        console.source = Some(name.clone());
                    }
                }

                if version.is_empty() {
                    self.success(format!("Connected to {name}"));
                } else {
                    self.success(format!("Connected to {name} — {version}"));
                }
            }

            Message::ConnectFailed { node, name, error } => {
                self.tree.set_failed(node, error.clone());
                self.error(format!("{name}: {error}"));
            }

            Message::Catalogs { node, catalogs } => {
                let children: Vec<Node> = catalogs
                    .iter()
                    .map(|catalog| self.tree.catalog_node(&catalog.name, catalog.is_current))
                    .collect();
                self.tree.set_children(node, children);

                // Drop straight into the database the connection is attached
                // to; it is nearly always the one being worked in.
                if let Some(current) = catalogs.iter().position(|catalog| catalog.is_current)
                    && let Some(child) = self.tree.find(node).and_then(|n| n.children.get(current))
                {
                    let child = child.id;
                    self.tree.select_id(child);
                    self.load_children(child);
                }
            }

            Message::Schemas { node, schemas } => {
                let children: Vec<Node> = schemas
                    .iter()
                    .map(|schema| self.tree.schema_node(schema))
                    .collect();
                self.tree.set_children(node, children);
            }

            Message::Objects { node, objects } => {
                let children = self.group_objects(objects);
                self.tree.set_children(node, children);
            }

            Message::Columns { node, columns } => {
                let children: Vec<Node> = columns
                    .iter()
                    .map(|column| self.tree.column_node(column.clone()))
                    .collect();
                self.tree.set_children(node, children);
            }

            Message::LoadFailed { node, error } => {
                self.tree.set_failed(node, error.clone());
                self.error(error);
            }

            Message::QueryFinished {
                console,
                generation,
                result,
            } => self.finish_query(console, generation, result),
        }
    }

    /// Splits a schema's objects into Tables and Views. The whole schema is
    /// fetched in one call, so expanding either group is instant afterwards.
    fn group_objects(&mut self, objects: Vec<ObjectRef>) -> Vec<Node> {
        let mut groups = Vec::new();
        for kind in [ObjectKind::Table, ObjectKind::View] {
            let members: Vec<ObjectRef> = objects
                .iter()
                .filter(|object| object.kind == kind)
                .cloned()
                .collect();
            if members.is_empty() {
                continue;
            }
            let children: Vec<Node> = members
                .into_iter()
                .map(|object| self.tree.object_node(object))
                .collect();
            let mut group = self.tree.group_node(kind);
            group.children = children;
            group.state = LoadState::Loaded;
            group.expanded = kind == ObjectKind::Table;
            groups.push(group);
        }
        groups
    }

    fn finish_query(&mut self, id: u64, generation: u64, result: Result<ResultSet, Error>) {
        let Some(console) = self.consoles.iter_mut().find(|console| console.id == id) else {
            return;
        };
        if console.generation != generation {
            return;
        }
        console.cancel = None;

        match result {
            Ok(result) if result.is_empty() && result.rows_affected.is_some() => {
                let count = result.rows_affected.unwrap_or(0);
                console.outcome = Outcome::Affected {
                    count,
                    elapsed: result.elapsed,
                };
                let rows = if count == 1 { "row" } else { "rows" };
                self.success(format!(
                    "{count} {rows} affected in {}",
                    format_elapsed(result.elapsed)
                ));
            }
            Ok(result) => {
                let count = result.rows.len();
                let elapsed = result.elapsed;
                let truncated = result.truncated;
                console.outcome = Outcome::Rows(Grid::new(result));
                let suffix = if truncated {
                    format!(" (first {count}, more available)")
                } else {
                    String::new()
                };
                let noun = if count == 1 { "row" } else { "rows" };
                self.success(format!(
                    "{count} {noun} in {}{suffix}",
                    format_elapsed(elapsed)
                ));
            }
            Err(Error::Cancelled) => {
                console.outcome = Outcome::Cancelled;
                self.warn("Query cancelled");
            }
            Err(error) => {
                let error = error.to_string();
                console.outcome = Outcome::Error(error.clone());
                self.error(error);
            }
        }
    }

    // --- data sources ---

    /// Files the secret, writes the config and opens the connection. Errors
    /// come back rather than being reported, so the form can stay up holding
    /// what was typed.
    ///
    /// The credential store is written before the config, so a config can never
    /// end up naming a secret that was never stored.
    pub fn save_data_source(&mut self, saved: Saved) -> Result<(), String> {
        let Saved {
            id,
            source,
            secret,
            previous,
            scope,
        } = saved;
        let renamed_from = previous.filter(|previous| *previous != id);

        match &secret {
            Some(secret) => keychain::set(&id, secret).map_err(|error| error.to_string())?,
            // Nothing new to file, so a rename carries the existing entry
            // across rather than leaving the new name pointing at nothing.
            None if keychain::is_reference(&source.dsn) => {
                if let Some(from) = &renamed_from {
                    keychain::rename(from, &id).map_err(|error| error.to_string())?;
                }
            }
            None => {}
        }

        let is_new = self.config.get(&id).is_none();
        self.config
            .set(&id, source, scope)
            .map_err(|error| error.to_string())?;
        if let Some(from) = &renamed_from {
            // Never through `remove_data_source`: the old name's secret has
            // either been carried across or superseded by the one just filed,
            // so deleting it would take the live one.
            self.sessions.remove(from);
            self.config
                .remove(from)
                .map_err(|error| error.to_string())?;
            self.tree.remove_source(from);
        }

        if is_new {
            self.tree.add_source(id.clone());
        }
        self.success(format!("Saved {id}"));
        if let Some(node) = self.source_node_id(&id) {
            self.tree.select_id(node);
            self.connect(node, &id);
        }
        Ok(())
    }

    pub fn remove_data_source(&mut self, name: &str) {
        self.sessions.remove(name);
        let secret = self
            .config
            .get(name)
            .and_then(|source| keychain::account(&source.dsn))
            .map(str::to_string);

        let removed = match self.config.remove(name) {
            Ok(removed) => removed,
            Err(error) => {
                self.error(format!("Saving connections: {error}"));
                return;
            }
        };

        if removed {
            // Only after the config is written: a secret left behind by a
            // failed save is recoverable, one deleted for a connection that is
            // still listed is not.
            if let Some(account) = secret
                && let Err(error) = keychain::delete(&account)
            {
                self.warn(format!("Removed {name}, but {error}"));
            }
            self.tree.remove_source(name);
            self.warn(format!("Removed {name}"));
        }
    }

    /// The data source the user is currently working in — the selected tree
    /// node's, or the active console's.
    pub fn context_source(&self) -> Option<String> {
        self.tree
            .selected_id()
            .and_then(|id| self.tree.context(id))
            .and_then(|context| context.source)
            .or_else(|| self.console().source.clone())
    }
}

pub fn format_elapsed(elapsed: Duration) -> String {
    let millis = elapsed.as_secs_f64() * 1000.0;
    if millis < 1.0 {
        format!("{:.2}ms", millis)
    } else if millis < 1000.0 {
        format!("{:.0}ms", millis)
    } else {
        format!("{:.2}s", elapsed.as_secs_f64())
    }
}
