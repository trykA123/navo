use rusty_core::Location;
use rusty_structured::NodeId;

use super::{CHILD_LIMIT, Contents};

impl Contents {
    pub(in crate::window_view) fn open_below(&mut self, index: usize) -> Vec<(NodeId, Location)> {
        let Some(node) = self
            .node_at(index)
            .filter(|node| self.folders.contains_key(node))
        else {
            return Vec::new();
        };
        self.open_deep(node)
    }

    pub(in crate::window_view) fn close_below(&mut self, index: usize) {
        let Some(node) = self.node_at(index) else {
            return;
        };
        let inside: Vec<NodeId> = self
            .deep
            .iter()
            .copied()
            .filter(|at| self.is_within(*at, node))
            .collect();
        for at in inside {
            self.deep.remove(&at);
        }
        self.outline.move_to(index + 1);
        self.outline.close_subtree(&self.tree);
    }

    pub(super) fn go_deeper(&mut self, parent: NodeId) -> Vec<(NodeId, Location)> {
        if self.deep.contains(&parent) {
            self.open_deep(parent)
        } else {
            Vec::new()
        }
    }

    fn is_within(&self, node: NodeId, top: NodeId) -> bool {
        std::iter::successors(Some(node), |at| self.tree.parent(*at)).any(|at| at == top)
    }

    fn exact_row(&self, node: NodeId) -> Option<usize> {
        self.shown_rows().iter().position(|row| row.node == node)
    }

    fn open_deep(&mut self, node: NodeId) -> Vec<(NodeId, Location)> {
        let mut reads = Vec::new();
        let mut stack = vec![node];
        while let Some(at) = stack.pop() {
            if self.len() >= CHILD_LIMIT {
                break;
            }
            if !self.folders.contains_key(&at) {
                continue;
            }
            self.deep.insert(at);
            let Some(index) = self.exact_row(at) else {
                continue;
            };
            if !self.outline.is_open(at)
                && let Some(read) = self.toggle(index)
            {
                reads.push(read);
                continue;
            }
            if self.tree.lazy(at).is_none() {
                let children: Vec<NodeId> = self.tree.children(at).collect();
                stack.extend(children.into_iter().rev());
            }
        }
        reads
    }
}
