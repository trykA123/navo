use gpui_kit::{FocusHandle, Stateful};

use super::MarkdownView;
use crate::prelude::*;
use crate::tokens::Theme;
use crate::window_view::viewer::search::view_find::{
    Findable, ViewFind, close_find, contains_ignore_case, find_again, hit_tint, open_find,
};
use crate::window_view::viewer::{Back, OpenFind};

impl Findable for MarkdownView {
    type Hit = usize;

    fn find_slot(&mut self) -> &mut Option<ViewFind<Self::Hit>> {
        &mut self.find
    }

    fn hits_for(&self, query: &str) -> (Vec<Self::Hit>, usize) {
        let top = self.list.logical_scroll_top().item_ix;
        let hits: Vec<usize> = self
            .doc
            .blocks()
            .iter()
            .enumerate()
            .filter(|(_, block)| contains_ignore_case(self.doc.text(block), query))
            .map(|(index, _)| index)
            .collect();
        let start = hits.iter().position(|index| *index >= top).unwrap_or(0);
        (hits, start)
    }

    fn show_hit(&mut self, block: Self::Hit, cx: &mut Context<Self>) {
        self.list.scroll_to_reveal_item(block);
        cx.notify();
    }

    fn body_focus(&self) -> FocusHandle {
        self.focus.clone()
    }
}

impl MarkdownView {
    pub(super) fn find_keys(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &OpenFind, window, cx| {
            if view.is_preview {
                cx.propagate();
            } else {
                open_find(view, window, cx);
            }
        }))
        .on_action(cx.listener(|view, _: &Back, window, cx| {
            if !close_find(view, window, cx) {
                cx.propagate();
            }
        }))
    }

    pub(super) fn find_next(
        &mut self,
        backward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        find_again(self, backward, window, cx);
    }

    pub(super) fn tint_found(&self, t: &Theme, block: usize, shown: AnyElement) -> AnyElement {
        let Some(find) = self.find.as_ref().filter(|find| find.is_hit(block)) else {
            return shown;
        };
        div()
            .w_full()
            .bg(hit_tint(t, find.is_current(block)))
            .child(shown)
            .into_any_element()
    }
}
