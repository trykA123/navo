use std::collections::HashMap;

use rusty_core::Location;
use rusty_fs::{EntryKind, FsEntry};
use rusty_structured::tree::NewNode;
use rusty_structured::{Flavor, Key, Lazy, NodeId, NodeKind, Outline, Row, Span, Step, Tree};

pub(super) const CHILD_LIMIT: usize = 2_000;
const UNREAD: Lazy = Lazy {
    span: Span { start: 0, end: 0 },
    count: 1,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::window_view) enum RowKind<'a> {
    Entry(&'a FsEntry),
    More(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::window_view) struct Shape {
    pub depth: u16,
    pub is_open: bool,
    pub can_open: bool,
}

pub(in crate::window_view) struct Place {
    pub folder: Location,
    pub name: String,
    pub is_folder: bool,
}

pub(in crate::window_view) struct Contents {
    tree: Tree,
    outline: Outline,
    entries: HashMap<NodeId, FsEntry>,
    folders: HashMap<NodeId, Location>,
    more: HashMap<NodeId, usize>,
    failure: Option<String>,
    is_read: bool,
}

fn node<'a>(kind: NodeKind, name: &'a str) -> NewNode<'a> {
    NewNode {
        kind,
        key: Key::Name(name),
        value: "",
        attrs: "",
    }
}

impl Contents {
    pub(super) fn new(folder: Location) -> Self {
        let mut tree = Tree::new(Flavor::Json);
        let root = tree.push(
            None,
            NewNode {
                key: Key::Root,
                ..node(NodeKind::Object, "")
            },
        );
        let mut folders = HashMap::new();
        if let Some(root) = root {
            tree.set_lazy(root, UNREAD);
            folders.insert(root, folder);
        }
        Self {
            outline: Outline::new(&tree),
            tree,
            entries: HashMap::new(),
            folders,
            more: HashMap::new(),
            failure: None,
            is_read: false,
        }
    }

    pub(super) fn root(&self) -> Option<(NodeId, Location)> {
        let root = self.tree.root()?;
        Some((root, self.folders.get(&root)?.clone()))
    }

    pub(super) fn is_read(&self) -> bool {
        self.is_read
    }

    pub(super) fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }

    pub(super) fn top_count(&self) -> usize {
        self.tree
            .root()
            .map_or(0, |root| self.tree.children(root).count())
    }

    pub(super) fn top_folders(&self) -> usize {
        self.tree.root().map_or(0, |root| {
            self.tree
                .children(root)
                .filter(|child| self.folders.contains_key(child))
                .count()
        })
    }

    fn shown_rows(&self) -> &[Row] {
        self.outline.rows().get(1..).unwrap_or_default()
    }

    pub(super) fn len(&self) -> usize {
        self.shown_rows().len()
    }

    pub(super) fn row(&self, index: usize) -> Option<(RowKind<'_>, Shape)> {
        let row = *self.shown_rows().get(index)?;
        let kind = match self.more.get(&row.node) {
            Some(hidden) => RowKind::More(*hidden),
            None => RowKind::Entry(self.entries.get(&row.node)?),
        };
        let shape = Shape {
            depth: row.depth.saturating_sub(1),
            is_open: self.outline.is_open(row.node),
            can_open: self.tree.child_count(row.node) > 0,
        };
        Some((kind, shape))
    }

    pub(super) fn location(&self, index: usize) -> Option<&Location> {
        let row = self.shown_rows().get(index)?;
        self.folders.get(&row.node)
    }

    pub(super) fn place(&self, index: usize) -> Option<Place> {
        let row = self.shown_rows().get(index)?;
        let entry = self.entries.get(&row.node)?;
        let parent = self.tree.parent(row.node)?;
        Some(Place {
            folder: self.folders.get(&parent)?.clone(),
            name: entry.name.to_text(),
            is_folder: self.folders.contains_key(&row.node),
        })
    }

    pub(super) fn parent_row(&self, index: usize) -> Option<usize> {
        let rows = self.shown_rows();
        let depth = rows.get(index)?.depth;
        rows.get(..index)?.iter().rposition(|row| row.depth < depth)
    }

    pub(super) fn toggle(&mut self, index: usize) -> Option<(NodeId, Location)> {
        match self.outline.toggle(&self.tree, index + 1) {
            Step::Done => None,
            Step::Load(node, _) => Some((node, self.folders.get(&node)?.clone())),
        }
    }

    pub(super) fn attach(&mut self, parent: NodeId, listed: Result<(Vec<FsEntry>, usize), String>) {
        let is_root = self.tree.root() == Some(parent);
        if self.tree.take_lazy(parent).is_none() {
            return;
        }
        match listed {
            Ok((entries, hidden)) => self.push_children(parent, entries, hidden),
            Err(message) if is_root => self.failure = Some(message),
            Err(_) => {}
        }
        self.is_read |= is_root;
        self.outline.loaded(&self.tree, parent);
    }

    fn push_children(&mut self, parent: NodeId, entries: Vec<FsEntry>, hidden: usize) {
        let Some(at) = self.folders.get(&parent).cloned() else {
            return;
        };
        for entry in entries {
            let name = entry.name.to_text();
            let is_folder = entry.kind == EntryKind::Folder;
            let kind = if is_folder {
                NodeKind::Object
            } else {
                NodeKind::String
            };
            let Some(id) = self.tree.push(Some(parent), node(kind, &name)) else {
                return;
            };
            if let Some(location) = is_folder.then(|| at.join(&entry.name).ok()).flatten() {
                self.tree.set_lazy(id, UNREAD);
                self.folders.insert(id, location);
            }
            self.entries.insert(id, entry);
        }
        if hidden > 0
            && let Some(id) = self.tree.push(Some(parent), node(NodeKind::Null, ""))
        {
            self.more.insert(id, hidden);
        }
    }
}

#[cfg(test)]
mod tests;
