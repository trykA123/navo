mod export;
mod filter;
mod outline;
mod path;

use std::collections::HashMap;

pub use export::{Exported, Syntax};
pub use filter::Filtered;
pub use outline::{Outline, Row, Step};

use crate::preview::{KEY_CHARS, PREVIEW_CHARS, clip, one_line};

pub type NodeId = u32;
const NONE: NodeId = NodeId::MAX;
pub const MAX_NODES: usize = 4_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Object,
    Array,
    String,
    Number,
    Bool,
    Null,
    Element,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavor {
    Json,
    Xml,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: u64,
    pub end: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lazy {
    pub span: Span,
    pub count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key<'a> {
    Root,
    Name(&'a str),
    Index(u32),
}

#[derive(Debug, Clone, Copy)]
pub struct NewNode<'a> {
    pub kind: NodeKind,
    pub key: Key<'a>,
    pub value: &'a str,
    pub attrs: &'a str,
}

#[derive(Debug, Clone, Copy, Default)]
struct Text {
    at: u32,
    len: u32,
}

#[derive(Debug, Clone, Copy)]
enum KeyRef {
    Root,
    Name(Text),
    Index(u32),
}

#[derive(Debug, Clone, Copy)]
struct Node {
    parent: NodeId,
    kind: NodeKind,
    key: KeyRef,
    value: Text,
    attrs: Text,
    first: NodeId,
    last: NodeId,
    next: NodeId,
    count: u32,
}

#[derive(Debug, Clone)]
pub struct Tree {
    flavor: Flavor,
    root: NodeId,
    nodes: Vec<Node>,
    text: String,
    lazy: HashMap<NodeId, Lazy>,
}

impl Tree {
    #[must_use]
    pub fn new(flavor: Flavor) -> Self {
        Self {
            flavor,
            root: NONE,
            nodes: Vec::new(),
            text: String::new(),
            lazy: HashMap::new(),
        }
    }

    #[must_use]
    pub fn flavor(&self) -> Flavor {
        self.flavor
    }

    #[must_use]
    pub fn root(&self) -> Option<NodeId> {
        (self.root != NONE).then_some(self.root)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    fn store(&mut self, text: &str) -> Text {
        let (Ok(at), Ok(len)) = (u32::try_from(self.text.len()), u32::try_from(text.len())) else {
            return Text::default();
        };
        if at.checked_add(len).is_none() {
            return Text::default();
        }
        self.text.push_str(text);
        Text { at, len }
    }

    fn text(&self, text: Text) -> &str {
        let start = text.at as usize;
        self.text
            .get(start..start + text.len as usize)
            .unwrap_or_default()
    }

    fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id as usize)
    }

    pub fn push(&mut self, parent: Option<NodeId>, new: NewNode<'_>) -> Option<NodeId> {
        if self.nodes.len() >= MAX_NODES {
            return None;
        }
        let id = NodeId::try_from(self.nodes.len()).ok()?;
        let key = match new.key {
            Key::Root => KeyRef::Root,
            Key::Index(index) => KeyRef::Index(index),
            Key::Name(name) => KeyRef::Name(self.store(clip(name, KEY_CHARS).0)),
        };
        let value = self.store(&one_line(new.value, PREVIEW_CHARS));
        let attrs = self.store(&one_line(new.attrs, PREVIEW_CHARS));
        let parent = parent.filter(|parent| self.node(*parent).is_some());
        self.nodes.push(Node {
            parent: parent.unwrap_or(NONE),
            kind: new.kind,
            key,
            value,
            attrs,
            first: NONE,
            last: NONE,
            next: NONE,
            count: 0,
        });
        match parent {
            Some(parent) => self.link(parent, id),
            None if self.root == NONE => self.root = id,
            None => {}
        }
        Some(id)
    }

    fn link(&mut self, parent: NodeId, child: NodeId) {
        let Some(last) = self.node(parent).map(|node| node.last) else {
            return;
        };
        if let Some(node) = self.nodes.get_mut(last as usize) {
            node.next = child;
        }
        if let Some(node) = self.nodes.get_mut(parent as usize) {
            if node.first == NONE {
                node.first = child;
            }
            node.last = child;
            node.count = node.count.saturating_add(1);
        }
    }

    pub fn set_value(&mut self, id: NodeId, value: &str) {
        let text = self.store(&one_line(value, PREVIEW_CHARS));
        if let Some(node) = self.nodes.get_mut(id as usize) {
            node.value = text;
        }
    }

    pub fn set_lazy(&mut self, id: NodeId, lazy: Lazy) {
        if lazy.count > 0 {
            self.lazy.insert(id, lazy);
        }
    }

    pub fn take_lazy(&mut self, id: NodeId) -> Option<Lazy> {
        self.lazy.remove(&id)
    }

    pub fn set_root(&mut self, id: NodeId) {
        if let Some(node) = self.nodes.get_mut(id as usize) {
            node.parent = NONE;
            node.key = KeyRef::Root;
            node.next = NONE;
            self.root = id;
        }
    }

    #[must_use]
    pub fn kind(&self, id: NodeId) -> NodeKind {
        self.node(id).map_or(NodeKind::Null, |node| node.kind)
    }

    #[must_use]
    pub fn key(&self, id: NodeId) -> Key<'_> {
        match self.node(id).map(|node| node.key) {
            Some(KeyRef::Name(text)) => Key::Name(self.text(text)),
            Some(KeyRef::Index(index)) => Key::Index(index),
            Some(KeyRef::Root) | None => Key::Root,
        }
    }

    #[must_use]
    pub fn value(&self, id: NodeId) -> &str {
        self.node(id).map_or("", |node| self.text(node.value))
    }

    #[must_use]
    pub fn attrs(&self, id: NodeId) -> &str {
        self.node(id).map_or("", |node| self.text(node.attrs))
    }

    #[must_use]
    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.node(id)
            .map(|node| node.parent)
            .filter(|parent| *parent != NONE)
    }

    pub fn children(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        let first = self.node(id).map_or(NONE, |node| node.first);
        std::iter::successors((first != NONE).then_some(first), |at| {
            self.node(*at)
                .map(|node| node.next)
                .filter(|next| *next != NONE)
        })
    }

    #[must_use]
    pub fn child_count(&self, id: NodeId) -> u32 {
        match self.lazy.get(&id) {
            Some(lazy) => lazy.count,
            None => self.node(id).map_or(0, |node| node.count),
        }
    }

    #[must_use]
    pub fn lazy(&self, id: NodeId) -> Option<Lazy> {
        self.lazy.get(&id).copied()
    }

    #[must_use]
    pub fn path(&self, id: NodeId) -> String {
        path::path(self, id)
    }

    pub fn ids(&self) -> impl Iterator<Item = NodeId> {
        (0..self.nodes.len()).filter_map(|id| NodeId::try_from(id).ok())
    }

    #[must_use]
    pub fn size_bytes(&self) -> usize {
        self.nodes.capacity() * std::mem::size_of::<Node>()
            + self.text.capacity()
            + self.lazy.capacity() * std::mem::size_of::<(NodeId, Lazy)>()
    }
}

#[cfg(test)]
mod export_tests;
#[cfg(test)]
mod tests;
