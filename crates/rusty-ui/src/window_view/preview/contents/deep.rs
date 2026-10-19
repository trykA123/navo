use std::collections::{HashSet, VecDeque};

use rusty_core::Location;
use rusty_structured::NodeId;

use super::{CHILD_LIMIT, Contents};

const DEEP_READS: usize = 4;

#[derive(Default)]
pub(super) struct Deep {
    nodes: HashSet<NodeId>,
    waiting: VecDeque<NodeId>,
    reading: HashSet<NodeId>,
    is_capped: bool,
}

impl Contents {
    pub(in crate::window_view) fn open_below(&mut self, index: usize) -> Vec<(NodeId, Location)> {
        let Some(node) = self
            .node_at(index)
            .filter(|node| self.folders.contains_key(node))
        else {
            return Vec::new();
        };
        self.deep.is_capped = false;
        self.walk_deep(node);
        self.next_deep_reads()
    }

    pub(in crate::window_view) fn close_below(&mut self, index: usize) {
        let Some(node) = self.node_at(index) else {
            return;
        };
        let inside: Vec<NodeId> = self
            .deep
            .nodes
            .iter()
            .copied()
            .filter(|at| self.is_within(*at, node))
            .collect();
        for at in inside {
            self.deep.nodes.remove(&at);
        }
        self.deep.waiting.clear();
        self.outline.move_to(index + 1);
        self.outline.close_subtree(&self.tree);
    }

    pub(super) fn cap_deep(&mut self, parent: NodeId) -> bool {
        self.deep.reading.remove(&parent);
        if !self.deep.nodes.contains(&parent) {
            return false;
        }
        let shown = self.len() + self.tree.child_count(parent) as usize;
        if !self.deep.is_capped && shown <= CHILD_LIMIT {
            return false;
        }
        self.deep.is_capped = true;
        self.deep.waiting.clear();
        self.deep.nodes.remove(&parent);
        if let Some(index) = self.exact_row(parent)
            && self.outline.is_open(parent)
        {
            self.outline.toggle(&self.tree, index + 1);
        }
        true
    }

    pub(super) fn go_deeper(&mut self, parent: NodeId) -> Vec<(NodeId, Location)> {
        if self.deep.is_capped || !self.deep.nodes.contains(&parent) {
            return Vec::new();
        }
        self.walk_deep(parent);
        self.next_deep_reads()
    }

    fn is_within(&self, node: NodeId, top: NodeId) -> bool {
        std::iter::successors(Some(node), |at| self.tree.parent(*at)).any(|at| at == top)
    }

    fn exact_row(&self, node: NodeId) -> Option<usize> {
        self.shown_rows().iter().position(|row| row.node == node)
    }

    fn walk_deep(&mut self, node: NodeId) {
        let mut stack = vec![node];
        while let Some(at) = stack.pop() {
            if self.len() >= CHILD_LIMIT {
                self.deep.is_capped = true;
                self.deep.waiting.clear();
                return;
            }
            if !self.folders.contains_key(&at) {
                continue;
            }
            self.deep.nodes.insert(at);
            if self.tree.lazy(at).is_some() {
                self.wait_for(at);
                continue;
            }
            if !self.outline.is_open(at)
                && let Some(index) = self.exact_row(at)
            {
                self.toggle(index);
            }
            let children: Vec<NodeId> = self.tree.children(at).collect();
            stack.extend(children.into_iter().rev());
        }
    }

    fn wait_for(&mut self, node: NodeId) {
        let is_known = self.outline.is_open(node) || self.deep.waiting.contains(&node);
        if !is_known {
            self.deep.waiting.push_back(node);
        }
    }

    fn next_deep_reads(&mut self) -> Vec<(NodeId, Location)> {
        let mut reads = Vec::new();
        while self.deep.reading.len() < DEEP_READS && !self.deep.is_capped {
            let Some(at) = self.deep.waiting.pop_front() else {
                break;
            };
            let Some(index) = self.exact_row(at).filter(|_| !self.outline.is_open(at)) else {
                continue;
            };
            if let Some(read) = self.toggle(index) {
                self.deep.reading.insert(at);
                reads.push(read);
            }
        }
        reads
    }
}
