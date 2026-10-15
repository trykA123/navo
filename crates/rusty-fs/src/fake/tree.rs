use rusty_core::{FsErrorKind, ProviderPath};

use crate::{Attributes, EntryKind, FsEntry};

#[derive(Debug)]
pub(super) struct Tree {
    root: Node,
}

#[derive(Debug)]
struct Node {
    entry: FsEntry,
    children: Vec<Node>,
}

impl Node {
    fn folder(name: ProviderPath) -> Self {
        Self::leaf(FsEntry {
            name,
            kind: EntryKind::Folder,
            size: 0,
            modified_ms: None,
            attributes: Attributes::default(),
        })
    }

    fn leaf(entry: FsEntry) -> Self {
        Self {
            entry,
            children: Vec::new(),
        }
    }

    fn child(&self, name: &ProviderPath) -> Option<&Self> {
        self.children.iter().find(|c| &c.entry.name == name)
    }

    fn child_mut(&mut self, name: &ProviderPath) -> Option<&mut Self> {
        self.children.iter_mut().find(|c| &c.entry.name == name)
    }

    fn child_or_folder(&mut self, name: &ProviderPath) -> &mut Self {
        let index = match self.children.iter().position(|c| &c.entry.name == name) {
            Some(index) => index,
            None => {
                self.children.push(Self::folder(name.clone()));
                self.children.len() - 1
            }
        };
        &mut self.children[index]
    }
}

fn segments(path: &str) -> impl Iterator<Item = ProviderPath> + '_ {
    path.split('/')
        .filter(|s| !s.is_empty())
        .map(ProviderPath::from_text)
}

pub(super) fn split_last(path: &str) -> Option<(&str, ProviderPath)> {
    let trimmed = path.trim_end_matches('/');
    let (parent, name) = trimmed.rsplit_once('/').unwrap_or(("", trimmed));
    (!name.is_empty()).then(|| (parent, ProviderPath::from_text(name)))
}

fn file(name: ProviderPath, size: u64) -> FsEntry {
    FsEntry {
        name,
        kind: EntryKind::File,
        size,
        modified_ms: None,
        attributes: Attributes::default(),
    }
}

impl Tree {
    pub(super) fn new() -> Self {
        Self {
            root: Node::folder(ProviderPath::from_text("/")),
        }
    }

    fn find(&self, path: &str) -> Option<&Node> {
        segments(path).try_fold(&self.root, |node, name| node.child(&name))
    }

    fn find_mut(&mut self, path: &str) -> Option<&mut Node> {
        segments(path).try_fold(&mut self.root, |node, name| node.child_mut(&name))
    }

    fn folder_mut(&mut self, path: &str) -> &mut Node {
        segments(path).fold(&mut self.root, |node, name| node.child_or_folder(&name))
    }

    pub(super) fn add_folder(&mut self, path: &str) {
        self.folder_mut(path);
    }

    pub(super) fn add_file(&mut self, path: &str, size: u64) {
        if let Some((parent, name)) = split_last(path) {
            self.insert(parent, file(name, size));
        }
    }

    pub(super) fn insert(&mut self, folder: &str, entry: FsEntry) {
        let parent = self.folder_mut(folder);
        match parent.child_mut(&entry.name) {
            Some(existing) => existing.entry = entry,
            None => parent.children.push(Node::leaf(entry)),
        }
    }

    pub(super) fn remove(&mut self, path: &str) {
        if let Some((parent, name)) = split_last(path)
            && let Some(node) = self.find_mut(parent)
        {
            node.children.retain(|c| c.entry.name != name);
        }
    }

    pub(super) fn is_folder(&self, path: &str) -> bool {
        self.find(path)
            .is_some_and(|node| node.entry.kind == EntryKind::Folder)
    }

    pub(super) fn has_children(&self, path: &str) -> bool {
        self.find(path)
            .is_some_and(|node| !node.children.is_empty())
    }

    pub(super) fn resize(&mut self, path: &str, size: u64) {
        if let Some(node) = self.find_mut(path) {
            node.entry.size = size;
        }
    }

    pub(super) fn move_node(&mut self, from: &str, to: &str) {
        let Some((parent, name)) = split_last(to) else {
            return;
        };
        if let Some(mut node) = self.take(from) {
            node.entry.name = name;
            self.folder_mut(parent).children.push(node);
        }
    }

    fn take(&mut self, path: &str) -> Option<Node> {
        let (parent, name) = split_last(path)?;
        let node = self.find_mut(parent)?;
        let index = node.children.iter().position(|c| c.entry.name == name)?;
        Some(node.children.remove(index))
    }

    pub(super) fn metadata(&self, path: &str) -> Option<FsEntry> {
        self.find(path).map(|node| node.entry.clone())
    }

    pub(super) fn children(&self, path: &str) -> Result<Vec<FsEntry>, FsErrorKind> {
        let node = self.find(path).ok_or(FsErrorKind::NotFound)?;
        if node.entry.kind != EntryKind::Folder {
            return Err(FsErrorKind::NameInvalid);
        }
        Ok(node.children.iter().map(|c| c.entry.clone()).collect())
    }
}
