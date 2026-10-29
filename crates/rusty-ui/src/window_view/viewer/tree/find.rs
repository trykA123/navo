use gpui_kit::{FocusHandle, Stateful};
use rusty_structured::{Key, NodeId, Step, Tree};

use super::TreeView;
use crate::prelude::*;
use crate::tokens::Theme;
use crate::window_view::viewer::OpenFind;
use crate::window_view::viewer::search::view_find::{
    Findable, ViewFind, close_find, contains_ignore_case, find_again, hit_tint, is_find_focused,
    open_find,
};

fn node_matches(tree: &Tree, node: NodeId, query: &str) -> bool {
    let key_matches = match tree.key(node) {
        Key::Name(name) => contains_ignore_case(name, query),
        Key::Index(index) => contains_ignore_case(&index.to_string(), query),
        Key::Root => false,
    };
    key_matches
        || contains_ignore_case(tree.value(node), query)
        || contains_ignore_case(tree.attrs(node), query)
}

fn display_order(tree: &Tree) -> Vec<NodeId> {
    let mut order = Vec::with_capacity(tree.len());
    let mut stack: Vec<NodeId> = tree.root().into_iter().collect();
    while let Some(node) = stack.pop() {
        order.push(node);
        let children: Vec<NodeId> = tree.children(node).collect();
        stack.extend(children.into_iter().rev());
    }
    order
}

impl Findable for TreeView {
    type Hit = NodeId;

    fn find_slot(&mut self) -> &mut Option<ViewFind<Self::Hit>> {
        &mut self.find
    }

    fn hits_for(&self, query: &str) -> (Vec<Self::Hit>, usize) {
        let Some(ready) = self.ready() else {
            return (Vec::new(), 0);
        };
        let selected = ready.outline.selected();
        let mut hits = Vec::new();
        let mut start = None;
        let mut is_after = selected.is_none();
        for node in display_order(&ready.tree) {
            is_after |= Some(node) == selected;
            if !node_matches(&ready.tree, node, query) {
                continue;
            }
            if is_after && start.is_none() {
                start = Some(hits.len());
            }
            hits.push(node);
        }
        (hits, start.unwrap_or(0))
    }

    fn show_hit(&mut self, node: Self::Hit, cx: &mut Context<Self>) {
        if self.is_filtering() {
            self.clear_filter(cx);
        }
        let Some(ready) = self.ready_mut() else {
            return;
        };
        let mut ancestors: Vec<NodeId> =
            std::iter::successors(ready.tree.parent(node), |at| ready.tree.parent(*at)).collect();
        ancestors.reverse();
        for ancestor in ancestors {
            if ready.outline.is_open(ancestor) {
                continue;
            }
            if let Some(at) = ready
                .outline
                .rows()
                .iter()
                .position(|row| row.node == ancestor)
            {
                ready.outline.toggle(&ready.tree, at);
            }
        }
        if let Some(at) = ready.outline.rows().iter().position(|row| row.node == node) {
            ready.outline.move_to(at);
        }
        self.follow(Step::Done, cx);
    }

    fn body_focus(&self) -> FocusHandle {
        self.focus.clone()
    }
}

impl TreeView {
    pub(super) fn find_keys(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &OpenFind, window, cx| open_find(view, window, cx)))
    }

    pub(super) fn find_next(
        &mut self,
        backward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        find_again(self, backward, window, cx);
    }

    pub(super) fn close_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        close_find(self, window, cx)
    }

    pub(super) fn is_typing_in_bar(&self, window: &Window, cx: &App) -> bool {
        is_find_focused(self.find.as_ref(), window, cx)
    }

    pub(super) fn tint_found(&self, t: &Theme, node: NodeId, row: AnyElement) -> AnyElement {
        let Some(find) = self.find.as_ref().filter(|find| find.is_hit(node)) else {
            return row;
        };
        div()
            .size_full()
            .bg(hit_tint(t, find.is_current(node)))
            .child(row)
            .into_any_element()
    }
}
