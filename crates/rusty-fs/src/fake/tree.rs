use rusty_core::{FsErrorKind, ProviderPath};

use crate::{Attributes, EntryKind, FsEntry};

#[derive(Debug)]
pub(super) struct Tree {
    root: Node,
    fold: bool,
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

    fn index(&self, name: &ProviderPath, fold: bool) -> Option<usize> {
        self.children
            .iter()
            .position(|c| same_name(&c.entry.name, name, fold))
    }

    fn child(&self, name: &ProviderPath, fold: bool) -> Option<&Self> {
        self.index(name, fold).map(|i| &self.children[i])
    }

    fn child_mut(&mut self, name: &ProviderPath, fold: bool) -> Option<&mut Self> {
        let index = self.index(name, fold)?;
        Some(&mut self.children[index])
    }

    fn child_or_folder(&mut self, name: &ProviderPath, fold: bool) -> &mut Self {
        let index = match self.index(name, fold) {
            Some(index) => index,
            None => {
                self.children.push(Self::folder(name.clone()));
                self.children.len() - 1
            }
        };
        &mut self.children[index]
    }
}

fn fold_char(c: char) -> char {
    let mut upper = c.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(u), None) => u,
        _ => c,
    }
}

pub(super) fn fold_text(text: &str) -> String {
    text.chars().map(fold_char).collect()
}

fn same_name(a: &ProviderPath, b: &ProviderPath, fold: bool) -> bool {
    a == b || (fold && fold_text(&a.to_text()) == fold_text(&b.to_text()))
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
    pub(super) fn new(fold: bool) -> Self {
        Self {
            root: Node::folder(ProviderPath::from_text("/")),
            fold,
        }
    }

    pub(super) fn folds(&self) -> bool {
        self.fold
    }

    fn find(&self, path: &str) -> Option<&Node> {
        segments(path).try_fold(&self.root, |node, name| node.child(&name, self.fold))
    }

    fn find_mut(&mut self, path: &str) -> Option<&mut Node> {
        let fold = self.fold;
        segments(path).try_fold(&mut self.root, |node, name| node.child_mut(&name, fold))
    }

    fn folder_mut(&mut self, path: &str) -> &mut Node {
        let fold = self.fold;
        segments(path).fold(&mut self.root, |node, name| {
            node.child_or_folder(&name, fold)
        })
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
        let fold = self.fold;
        let parent = self.folder_mut(folder);
        match parent.child_mut(&entry.name, fold) {
            Some(existing) => existing.entry = entry,
            None => parent.children.push(Node::leaf(entry)),
        }
    }

    pub(super) fn remove(&mut self, path: &str) {
        let fold = self.fold;
        if let Some((parent, name)) = split_last(path)
            && let Some(node) = self.find_mut(parent)
        {
            node.children
                .retain(|c| !same_name(&c.entry.name, &name, fold));
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
        let fold = self.fold;
        let (parent, name) = split_last(path)?;
        let node = self.find_mut(parent)?;
        let index = node.index(&name, fold)?;
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
