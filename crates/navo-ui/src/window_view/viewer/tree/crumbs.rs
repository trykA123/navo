//! The breadcrumb over the tree view: one clickable segment per ancestor of the selected node.
//! Its accessible name is the node's full `Tree::path()`, the text Copy Path copies.

use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;
use navo_structured::{Key, NodeId, Step, Tree};

use super::TreeView;
use super::row::key_text;
use crate::prelude::*;
use crate::tokens::Theme;

/// The most segments drawn; deeper paths start with an ellipsis.
const MAX_CRUMBS: usize = 16;

/// The segment text: the key, `[n]` for an array index, `$` for a JSON root.
fn crumb_label(tree: &Tree, node: NodeId) -> String {
    match tree.key(node) {
        Key::Index(index) => format!("[{index}]"),
        _ => key_text(tree, node),
    }
}

fn separator(t: &Theme, text: &'static str) -> Div {
    div().flex_none().text_color(t.colors.faint).child(text)
}

/// What one breadcrumb segment shows and which node it selects.
struct CrumbLook<'a> {
    t: &'a Theme,
    label: String,
    index: usize,
    node: NodeId,
    is_last: bool,
}

impl TreeView {
    /// Selects `node` from a breadcrumb click and moves the focus back to the tree.
    fn select_crumb(&mut self, node: NodeId, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        let Some(ready) = self.ready_mut() else {
            return;
        };
        match ready.outline.rows().iter().position(|row| row.node == node) {
            Some(at) => {
                ready.outline.move_to(at);
                self.follow(Step::Done, cx);
            }
            None => self.reveal(node, cx),
        }
    }

    /// Draws one clickable segment that selects `node` (mouse parity for walking up the path).
    fn crumb(&self, look: CrumbLook<'_>, cx: &mut Context<Self>) -> impl IntoElement {
        let CrumbLook {
            t,
            label,
            index,
            node,
            is_last,
        } = look;
        let color = if is_last {
            t.colors.text
        } else {
            t.colors.accent_text
        };
        let hover = t.colors.hover;
        div()
            .id(("tree-crumb", index))
            .role(Role::Button)
            .aria_label(label.clone())
            .test_support()
            .flex()
            .flex_none()
            .items_center()
            .h(t.spacing.row_height)
            .px(t.spacing.xs)
            .rounded(t.radii.sm)
            .cursor_pointer()
            .text_color(color)
            .hover(move |style| style.bg(hover))
            .on_click(cx.listener(move |view, _, window, cx| {
                view.select_crumb(node, window, cx);
            }))
            .child(label)
    }

    /// Draws the breadcrumb of the selected node; None while no node is selected.
    pub(super) fn crumbs_view(
        &self,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        let ready = self.ready()?;
        let path = self.path.clone()?;
        let node = ready.outline.selected()?;
        let mut chain: Vec<NodeId> =
            std::iter::successors(Some(node), |at| ready.tree.parent(*at)).collect();
        chain.reverse();
        let skipped = chain.len().saturating_sub(MAX_CRUMBS);
        let last = chain.len().saturating_sub(1);
        let mut row = div()
            .id("tree-path")
            .aria_label(path)
            .test_support()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.xxs)
            .px(t.spacing.xl)
            .h(t.spacing.path_height)
            .overflow_hidden()
            .border_b_1()
            .border_color(t.colors.line)
            .bg(t.colors.rail)
            .font_family(t.typography.mono_font.clone())
            .text_size(t.typography.small)
            .when(skipped > 0, |row| row.child(separator(t, "…")));
        for (index, at) in chain.iter().copied().enumerate().skip(skipped) {
            if index > skipped {
                row = row.child(separator(t, "›"));
            }
            let look = CrumbLook {
                t,
                label: crumb_label(&ready.tree, at),
                index,
                node: at,
                is_last: index == last,
            };
            row = row.child(self.crumb(look, cx));
        }
        Some(row)
    }
}
