use super::Outline;
use crate::tree::{NodeId, Tree};

impl Outline {
    pub fn expand_all(&mut self, tree: &Tree) {
        let Some(root) = tree.root().filter(|_| self.filter.is_empty()) else {
            return;
        };
        let keep = self.selected();
        self.open_loaded_below(tree, root);
        self.rebuild_near(tree, keep);
    }

    pub fn close_subtree(&mut self, tree: &Tree) {
        let Some(node) = self.selected().filter(|_| self.filter.is_empty()) else {
            return;
        };
        let mut stack = vec![node];
        while let Some(at) = stack.pop() {
            if self.is_open(at) {
                self.set_open(at, false);
            }
            stack.extend(tree.children(at));
        }
        if let Some(root) = tree.root() {
            self.set_open(root, true);
        }
        self.rebuild_near(tree, Some(node));
    }

    pub fn fold_to_depth(&mut self, tree: &Tree, depth: u16) {
        let Some(root) = tree.root().filter(|_| self.filter.is_empty()) else {
            return;
        };
        let keep = self.selected();
        self.open.fill(false);
        let mut stack = vec![(root, 0_u16)];
        while let Some((at, at_depth)) = stack.pop() {
            if at_depth < depth && tree.lazy(at).is_none() && tree.child_count(at) > 0 {
                self.set_open(at, true);
                stack.extend(tree.children(at).map(|child| (child, at_depth + 1)));
            }
        }
        self.set_open(root, true);
        self.rebuild_near(tree, keep);
    }

    pub(super) fn open_loaded_below(&mut self, tree: &Tree, node: NodeId) {
        let mut stack = vec![node];
        while let Some(at) = stack.pop() {
            if tree.lazy(at).is_none() && tree.child_count(at) > 0 {
                self.set_open(at, true);
                stack.extend(tree.children(at));
            }
        }
    }

    fn rebuild_near(&mut self, tree: &Tree, keep: Option<NodeId>) {
        let shown = keep.map(|node| self.nearest_shown(tree, node));
        self.rebuild(tree);
        self.cursor = shown.and_then(|node| self.row_of(node)).unwrap_or(0);
    }

    fn nearest_shown(&self, tree: &Tree, node: NodeId) -> NodeId {
        let ancestors: Vec<NodeId> =
            std::iter::successors(tree.parent(node), |at| tree.parent(*at)).collect();
        ancestors
            .iter()
            .rev()
            .find(|ancestor| !self.is_open(**ancestor))
            .copied()
            .unwrap_or(node)
    }
}

#[cfg(test)]
mod tests;
