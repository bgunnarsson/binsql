//! The database explorer's model.
//!
//! One tree holds every open data source, which is the whole point of the
//! rewrite: `local`, `staging` and `prod` sit side by side and each expands
//! into its own databases, schemas and tables.
//!
//! Levels are the same for every backend — source, catalog, schema, group,
//! object, column — and the ones a backend does not have are simply skipped
//! when its children are built, so nothing downstream special-cases MySQL for
//! having no schemas.

use binsql_core::{Column, ObjectKind, ObjectRef};

/// Stable across reloads, unlike a path, so a slow response cannot be applied
/// to whichever node has since taken its place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(u64);

#[derive(Debug, Clone)]
pub enum NodeKind {
    /// A folder of data sources — a client, a project. Purely a grouping: it
    /// holds no connection of its own and never loads anything.
    Folder {
        name: String,
    },
    Source {
        /// The qualified name, `eimskip/prod`, which is what identifies a
        /// session everywhere else. The tree shows only the leaf.
        name: String,
        connected: bool,
    },
    Catalog {
        name: String,
        is_current: bool,
    },
    Schema {
        name: String,
    },
    Group {
        kind: ObjectKind,
    },
    Object {
        object: ObjectRef,
    },
    ColumnNode {
        column: Column,
    },
    /// A leaf that carries a message rather than a database object — "no
    /// tables", or why a load failed.
    Note {
        text: String,
        is_error: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum LoadState {
    /// Children are not known yet; expanding asks for them.
    Pending,
    Loading,
    Loaded,
    Failed(String),
    /// Nothing to load — a column, or a note.
    Leaf,
}

#[derive(Debug)]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    pub children: Vec<Node>,
    pub expanded: bool,
    pub state: LoadState,
}

impl Node {
    pub fn is_expandable(&self) -> bool {
        !matches!(self.state, LoadState::Leaf)
    }
}

/// Where a node sits, assembled by walking down to it. Nodes do not each carry
/// a copy of their ancestors' names — the tree already knows them.
#[derive(Debug, Clone, Default)]
pub struct NodeContext {
    pub source: Option<String>,
    pub catalog: Option<String>,
    pub schema: Option<String>,
    pub object: Option<ObjectRef>,
}

#[derive(Debug, Clone)]
pub struct VisibleNode {
    pub id: NodeId,
    pub depth: usize,
}

#[derive(Debug, Default)]
pub struct Tree {
    pub roots: Vec<Node>,
    next_id: u64,
    pub selected: usize,
    pub offset: usize,
}

impl Tree {
    pub fn new() -> Tree {
        Tree::default()
    }

    fn make(&mut self, kind: NodeKind, state: LoadState) -> Node {
        self.next_id += 1;
        Node {
            id: NodeId(self.next_id),
            kind,
            children: Vec::new(),
            expanded: false,
            state,
        }
    }

    /// Adds a data source at the top level, or inside its folder — creating the
    /// folder if this is the first source to land in it.
    pub fn add_source(&mut self, id: impl Into<String>) -> NodeId {
        let id = id.into();
        let (folder, _) = binsql_core::config::split_qualified(&id);

        let node = self.make(
            NodeKind::Source {
                name: id.clone(),
                connected: false,
            },
            LoadState::Pending,
        );
        let node_id = node.id;

        match folder {
            None => self.roots.push(node),
            Some(folder) => match self.folder_mut(folder) {
                Some(existing) => existing.children.push(node),
                None => {
                    let mut created = self.make(
                        NodeKind::Folder {
                            name: folder.to_string(),
                        },
                        // A folder's children are its config entries; there is
                        // nothing behind it to fetch, so it starts loaded — but
                        // closed, so a machine with a dozen clients on it opens
                        // to a list of clients rather than every database at
                        // once.
                        LoadState::Loaded,
                    );
                    created.children.push(node);
                    self.roots.push(created);
                }
            },
        }
        node_id
    }

    fn folder_mut(&mut self, name: &str) -> Option<&mut Node> {
        self.roots
            .iter_mut()
            .find(|node| matches!(&node.kind, NodeKind::Folder { name: n } if n == name))
    }

    /// Removes a data source wherever it sits, and the folder it leaves empty.
    pub fn remove_source(&mut self, id: &str) {
        fn is_source(node: &Node, id: &str) -> bool {
            matches!(&node.kind, NodeKind::Source { name, .. } if name == id)
        }

        self.roots.retain(|node| !is_source(node, id));
        for folder in &mut self.roots {
            folder.children.retain(|node| !is_source(node, id));
        }
        self.roots.retain(|node| {
            !matches!(&node.kind, NodeKind::Folder { .. }) || !node.children.is_empty()
        });
        self.clamp_selection();
    }

    pub fn find(&self, id: NodeId) -> Option<&Node> {
        fn walk(nodes: &[Node], id: NodeId) -> Option<&Node> {
            for node in nodes {
                if node.id == id {
                    return Some(node);
                }
                if let Some(found) = walk(&node.children, id) {
                    return Some(found);
                }
            }
            None
        }
        walk(&self.roots, id)
    }

    pub fn find_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        fn walk(nodes: &mut [Node], id: NodeId) -> Option<&mut Node> {
            for node in nodes {
                if node.id == id {
                    return Some(node);
                }
                if let Some(found) = walk(&mut node.children, id) {
                    return Some(found);
                }
            }
            None
        }
        walk(&mut self.roots, id)
    }

    /// Finds a data source by its qualified name, at the top level or in a
    /// folder.
    pub fn source_node(&self, id: &str) -> Option<&Node> {
        self.roots
            .iter()
            .flat_map(|node| std::iter::once(node).chain(node.children.iter()))
            .find(|node| matches!(&node.kind, NodeKind::Source { name, .. } if name == id))
    }

    pub fn context(&self, id: NodeId) -> Option<NodeContext> {
        fn walk(nodes: &[Node], id: NodeId, context: NodeContext) -> Option<NodeContext> {
            for node in nodes {
                let mut here = context.clone();
                match &node.kind {
                    NodeKind::Folder { .. } => {}
                    NodeKind::Source { name, .. } => here.source = Some(name.clone()),
                    NodeKind::Catalog { name, .. } => here.catalog = Some(name.clone()),
                    NodeKind::Schema { name } => here.schema = Some(name.clone()),
                    NodeKind::Object { object } => here.object = Some(object.clone()),
                    NodeKind::Group { .. }
                    | NodeKind::ColumnNode { .. }
                    | NodeKind::Note { .. } => {}
                }
                if node.id == id {
                    return Some(here);
                }
                if let Some(found) = walk(&node.children, id, here) {
                    return Some(found);
                }
            }
            None
        }
        walk(&self.roots, id, NodeContext::default())
    }

    /// The rows the explorer draws, in order, with their indentation depth.
    pub fn visible(&self) -> Vec<VisibleNode> {
        fn walk(nodes: &[Node], depth: usize, out: &mut Vec<VisibleNode>) {
            for node in nodes {
                out.push(VisibleNode { id: node.id, depth });
                if node.expanded {
                    walk(&node.children, depth + 1, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.roots, 0, &mut out);
        out
    }

    pub fn selected_id(&self) -> Option<NodeId> {
        self.visible().get(self.selected).map(|node| node.id)
    }

    pub fn select_id(&mut self, id: NodeId) {
        if let Some(index) = self.visible().iter().position(|node| node.id == id) {
            self.selected = index;
        }
    }

    pub fn move_selection(&mut self, delta: isize) {
        let count = self.visible().len();
        if count == 0 {
            return;
        }
        let next = self.selected as isize + delta;
        self.selected = next.clamp(0, count as isize - 1) as usize;
    }

    pub fn select_first(&mut self) {
        self.selected = 0;
    }

    pub fn select_last(&mut self) {
        self.selected = self.visible().len().saturating_sub(1);
    }

    fn clamp_selection(&mut self) {
        let count = self.visible().len();
        self.selected = self.selected.min(count.saturating_sub(1));
    }

    /// Collapses the selected node, or steps to its parent when it is already
    /// collapsed — the movement people expect from a file tree.
    pub fn collapse_or_parent(&mut self) {
        let Some(id) = self.selected_id() else {
            return;
        };
        let expanded = self.find(id).is_some_and(|node| node.expanded);
        if expanded {
            if let Some(node) = self.find_mut(id) {
                node.expanded = false;
            }
            return;
        }
        if let Some(parent) = self.parent_of(id) {
            self.select_id(parent);
        }
    }

    fn parent_of(&self, id: NodeId) -> Option<NodeId> {
        fn walk(nodes: &[Node], id: NodeId, parent: Option<NodeId>) -> Option<NodeId> {
            for node in nodes {
                if node.id == id {
                    return parent;
                }
                if let Some(found) = walk(&node.children, id, Some(node.id)) {
                    return Some(found);
                }
            }
            None
        }
        walk(&self.roots, id, None)
    }

    /// Replaces a node's children and marks it loaded.
    pub fn set_children(&mut self, id: NodeId, children: Vec<Node>) {
        let children = if children.is_empty() {
            vec![self.note("(empty)", false)]
        } else {
            children
        };
        if let Some(node) = self.find_mut(id) {
            node.children = children;
            node.state = LoadState::Loaded;
            node.expanded = true;
        }
    }

    pub fn set_failed(&mut self, id: NodeId, error: impl Into<String>) {
        let error = error.into();
        let note = self.note(error.clone(), true);
        if let Some(node) = self.find_mut(id) {
            node.state = LoadState::Failed(error);
            node.children = vec![note];
            node.expanded = true;
        }
    }

    pub fn set_loading(&mut self, id: NodeId) {
        if let Some(node) = self.find_mut(id) {
            node.state = LoadState::Loading;
        }
    }

    pub fn mark_connected(&mut self, id: NodeId, connected: bool) {
        if let Some(node) = self.find_mut(id)
            && let NodeKind::Source {
                connected: flag, ..
            } = &mut node.kind
        {
            *flag = connected;
        }

        // Folders start closed, so a source that connects on its own — the
        // default, or one flagged `open_on_start` — would otherwise be working
        // away invisibly inside one.
        if connected
            && let Some(parent) = self.parent_of(id)
            && let Some(folder) = self.find_mut(parent)
            && matches!(folder.kind, NodeKind::Folder { .. })
        {
            folder.expanded = true;
        }
    }

    /// Drops a node's children so the next expansion reloads them.
    pub fn invalidate(&mut self, id: NodeId) {
        if let Some(node) = self.find_mut(id) {
            node.children.clear();
            node.expanded = false;
            if node.state != LoadState::Leaf {
                node.state = LoadState::Pending;
            }
        }
        self.clamp_selection();
    }

    pub fn note(&mut self, text: impl Into<String>, is_error: bool) -> Node {
        self.make(
            NodeKind::Note {
                text: text.into(),
                is_error,
            },
            LoadState::Leaf,
        )
    }

    pub fn catalog_node(&mut self, name: impl Into<String>, is_current: bool) -> Node {
        self.make(
            NodeKind::Catalog {
                name: name.into(),
                is_current,
            },
            LoadState::Pending,
        )
    }

    pub fn schema_node(&mut self, name: impl Into<String>) -> Node {
        self.make(NodeKind::Schema { name: name.into() }, LoadState::Pending)
    }

    pub fn group_node(&mut self, kind: ObjectKind) -> Node {
        self.make(NodeKind::Group { kind }, LoadState::Pending)
    }

    pub fn object_node(&mut self, object: ObjectRef) -> Node {
        self.make(NodeKind::Object { object }, LoadState::Pending)
    }

    pub fn column_node(&mut self, column: Column) -> Node {
        self.make(NodeKind::ColumnNode { column }, LoadState::Leaf)
    }
}
