use super::{Key, Lazy, NodeId, Tree};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub node: NodeId,
    pub depth: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Done,
    Load(NodeId, Lazy),
}

#[derive(Debug, Clone, Default)]
pub struct Outline {
    rows: Vec<Row>,
    open: Vec<bool>,
    cursor: usize,
    filter: String,
}

fn matches(key: Key<'_>, needle: &str) -> bool {
    let Key::Name(name) = key else {
        return false;
    };
    if needle.is_ascii() {
        let (hay, needle) = (name.as_bytes(), needle.as_bytes());
        return needle.is_empty()
            || hay
                .windows(needle.len())
                .any(|window| window.eq_ignore_ascii_case(needle));
    }
    name.to_lowercase().contains(&needle.to_lowercase())
}

impl Outline {
    #[must_use]
    pub fn new(tree: &Tree) -> Self {
        let mut outline = Self::default();
        if let Some(root) = tree.root() {
            outline.set_open(root, true);
            outline.rebuild(tree);
        }
        outline
    }

    #[must_use]
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    #[must_use]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    #[must_use]
    pub fn selected(&self) -> Option<NodeId> {
        self.rows.get(self.cursor).map(|row| row.node)
    }

    #[must_use]
    pub fn filter(&self) -> &str {
        &self.filter
    }

    #[must_use]
    pub fn is_open(&self, id: NodeId) -> bool {
        self.open.get(id as usize).copied().unwrap_or(false)
    }

    fn set_open(&mut self, id: NodeId, open: bool) {
        let at = id as usize;
        if at >= self.open.len() {
            self.open.resize(at + 1, false);
        }
        if let Some(slot) = self.open.get_mut(at) {
            *slot = open;
        }
    }

    pub fn move_to(&mut self, row: usize) {
        self.cursor = row.min(self.rows.len().saturating_sub(1));
    }

    pub fn move_by(&mut self, delta: i64) {
        let target = i64::try_from(self.cursor)
            .unwrap_or(i64::MAX)
            .saturating_add(delta)
            .max(0);
        self.move_to(usize::try_from(target).unwrap_or(usize::MAX));
    }

    fn visible_below(&self, tree: &Tree, node: NodeId, depth: u16) -> Vec<Row> {
        let mut rows = Vec::new();
        let mut stack: Vec<(NodeId, u16)> = Vec::new();
        let push_children = |stack: &mut Vec<(NodeId, u16)>, parent: NodeId, depth: u16| {
            let children: Vec<NodeId> = tree.children(parent).collect();
            stack.extend(
                children
                    .into_iter()
                    .rev()
                    .map(|child| (child, depth.saturating_add(1))),
            );
        };
        if self.is_open(node) {
            push_children(&mut stack, node, depth);
        }
        while let Some((at, at_depth)) = stack.pop() {
            rows.push(Row {
                node: at,
                depth: at_depth,
            });
            if self.is_open(at) {
                push_children(&mut stack, at, at_depth);
            }
        }
        rows
    }

    fn rebuild(&mut self, tree: &Tree) {
        self.rows.clear();
        if let Some(root) = tree.root() {
            self.rows.push(Row {
                node: root,
                depth: 0,
            });
            let below = self.visible_below(tree, root, 0);
            self.rows.extend(below);
        }
        self.move_to(self.cursor);
    }

    fn row_of(&self, node: NodeId) -> Option<usize> {
        self.rows.iter().position(|row| row.node == node)
    }

    fn open_row(&mut self, tree: &Tree, at: usize) -> Step {
        let Some(row) = self.rows.get(at).copied() else {
            return Step::Done;
        };
        if self.is_open(row.node) || tree.child_count(row.node) == 0 {
            return Step::Done;
        }
        self.set_open(row.node, true);
        if let Some(lazy) = tree.lazy(row.node) {
            return Step::Load(row.node, lazy);
        }
        let below = self.visible_below(tree, row.node, row.depth);
        self.rows.splice(at + 1..at + 1, below);
        Step::Done
    }

    fn close_row(&mut self, at: usize) {
        let Some(row) = self.rows.get(at).copied() else {
            return;
        };
        self.set_open(row.node, false);
        let end = self
            .rows
            .iter()
            .skip(at + 1)
            .position(|below| below.depth <= row.depth)
            .map_or(self.rows.len(), |offset| at + 1 + offset);
        self.rows.drain(at + 1..end);
        if self.cursor > at && self.cursor < end {
            self.cursor = at;
        } else if self.cursor >= end {
            self.cursor -= end - at - 1;
        }
    }

    fn parent_row(&self, at: usize) -> Option<usize> {
        let depth = self.rows.get(at)?.depth;
        self.rows
            .get(..at)?
            .iter()
            .rposition(|row| row.depth < depth)
    }

    fn first_child_row(&self, at: usize) -> Option<usize> {
        let depth = self.rows.get(at)?.depth;
        self.rows
            .get(at + 1)
            .filter(|row| row.depth > depth)
            .map(|_| at + 1)
    }

    pub fn left(&mut self, tree: &Tree) {
        let at = self.cursor;
        let Some(row) = self.rows.get(at).copied() else {
            return;
        };
        let can_close = self.filter.is_empty()
            && self.is_open(row.node)
            && tree.child_count(row.node) > 0
            && Some(row.node) != tree.root();
        if can_close {
            self.close_row(at);
        } else if let Some(parent) = self.parent_row(at) {
            self.cursor = parent;
        }
    }

    pub fn right(&mut self, tree: &Tree) -> Step {
        let at = self.cursor;
        let Some(row) = self.rows.get(at).copied() else {
            return Step::Done;
        };
        if self.filter.is_empty() && !self.is_open(row.node) {
            return self.open_row(tree, at);
        }
        if let Some(child) = self.first_child_row(at) {
            self.cursor = child;
        }
        Step::Done
    }

    pub fn toggle(&mut self, tree: &Tree, at: usize) -> Step {
        let Some(row) = self.rows.get(at).copied() else {
            return Step::Done;
        };
        self.cursor = at;
        if !self.filter.is_empty() || Some(row.node) == tree.root() && self.is_open(row.node) {
            return Step::Done;
        }
        if self.is_open(row.node) {
            self.close_row(at);
            Step::Done
        } else {
            self.open_row(tree, at)
        }
    }

    pub fn expand_subtree(&mut self, tree: &Tree) -> Step {
        let Some(node) = self.selected() else {
            return Step::Done;
        };
        if let Some(lazy) = tree.lazy(node) {
            self.set_open(node, true);
            return Step::Load(node, lazy);
        }
        let mut stack = vec![node];
        while let Some(at) = stack.pop() {
            if tree.lazy(at).is_none() && tree.child_count(at) > 0 {
                self.set_open(at, true);
                stack.extend(tree.children(at));
            }
        }
        self.rebuild_keeping(tree, node);
        Step::Done
    }

    pub fn collapse_all(&mut self, tree: &Tree) {
        let keep = self.selected();
        self.open.iter_mut().for_each(|open| *open = false);
        if let Some(root) = tree.root() {
            self.set_open(root, true);
        }
        let shown = keep.and_then(|node| self.top_ancestor(tree, node));
        self.rebuild(tree);
        self.cursor = shown.and_then(|node| self.row_of(node)).unwrap_or(0);
    }

    fn top_ancestor(&self, tree: &Tree, node: NodeId) -> Option<NodeId> {
        let root = tree.root()?;
        let mut at = node;
        while let Some(parent) = tree.parent(at) {
            if parent == root {
                return Some(at);
            }
            at = parent;
        }
        Some(root)
    }

    fn rebuild_keeping(&mut self, tree: &Tree, node: NodeId) {
        self.rebuild(tree);
        if let Some(row) = self.row_of(node) {
            self.cursor = row;
        }
    }

    pub fn loaded(&mut self, tree: &Tree, node: NodeId) {
        if !self.filter.is_empty() || !self.is_open(node) {
            return;
        }
        let Some(at) = self.row_of(node) else {
            return;
        };
        let depth = self.rows.get(at).map_or(0, |row| row.depth);
        let has_rows = self
            .rows
            .get(at + 1)
            .is_some_and(|below| below.depth > depth);
        if !has_rows {
            let below = self.visible_below(tree, node, depth);
            let added = below.len();
            self.rows.splice(at + 1..at + 1, below);
            if self.cursor > at {
                self.cursor += added;
            }
        }
    }

    pub fn set_filter(&mut self, tree: &Tree, text: &str) {
        let keep = self.selected();
        self.filter = text.to_owned();
        if self.filter.is_empty() {
            if let Some(node) = keep {
                self.reveal(tree, node);
            }
            self.rebuild(tree);
            self.cursor = keep.and_then(|node| self.row_of(node)).unwrap_or(0);
            return;
        }
        self.rows = filtered_rows(tree, &self.filter);
        self.cursor = self
            .rows
            .iter()
            .position(|row| matches(tree.key(row.node), &self.filter))
            .unwrap_or(0);
    }

    #[must_use]
    pub fn matching(&self, tree: &Tree) -> usize {
        if self.filter.is_empty() {
            return 0;
        }
        self.rows
            .iter()
            .filter(|row| matches(tree.key(row.node), &self.filter))
            .count()
    }

    fn reveal(&mut self, tree: &Tree, node: NodeId) {
        let mut at = node;
        while let Some(parent) = tree.parent(at) {
            self.set_open(parent, true);
            at = parent;
        }
    }
}

fn filtered_rows(tree: &Tree, filter: &str) -> Vec<Row> {
    let mut keep = vec![false; tree.len()];
    for id in tree.ids() {
        if !matches(tree.key(id), filter) {
            continue;
        }
        let mut at = Some(id);
        while let Some(node) = at {
            let Some(slot) = keep.get_mut(node as usize) else {
                break;
            };
            if *slot {
                break;
            }
            *slot = true;
            at = tree.parent(node);
        }
    }
    let Some(root) = tree.root() else {
        return Vec::new();
    };
    let is_kept = |id: NodeId| keep.get(id as usize).copied().unwrap_or(false);
    let mut rows = Vec::new();
    let mut stack = vec![(root, 0u16)];
    while let Some((at, depth)) = stack.pop() {
        if !is_kept(at) {
            continue;
        }
        rows.push(Row { node: at, depth });
        let children: Vec<NodeId> = tree.children(at).filter(|id| is_kept(*id)).collect();
        stack.extend(
            children
                .into_iter()
                .rev()
                .map(|child| (child, depth.saturating_add(1))),
        );
    }
    rows
}
