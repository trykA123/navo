use gpui_kit::{ClipboardItem, KeyBinding, KeyDownEvent, Stateful};

use super::{TreeState, TreeView};
use crate::file_list::typed_char;
use crate::prelude::*;
use crate::window_view::viewer::{
    Back, GoEnd, GoStart, PageDown, PageUp, RowDown, RowUp, ScrollLeft, ScrollRight,
};

pub(super) const CONTEXT: &str = "Tree";

gpui_kit::actions!(
    viewer_tree,
    [
        ExpandSubtree,
        CollapseAll,
        CopyPath,
        FilterBack,
        TypeN,
        TypeP,
        TypeOne,
        TypeThree,
        TypeFive,
    ]
);

pub(in crate::window_view::viewer) fn bind_keys(cx: &mut App) {
    let context = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("*", ExpandSubtree, context),
        KeyBinding::new("-", CollapseAll, context),
        KeyBinding::new("ctrl-shift-c", CopyPath, context),
        KeyBinding::new("backspace", FilterBack, context),
        KeyBinding::new("escape", Back, context),
        KeyBinding::new("home", GoStart, context),
        KeyBinding::new("end", GoEnd, context),
        KeyBinding::new("n", TypeN, context),
        KeyBinding::new("p", TypeP, context),
        KeyBinding::new("1", TypeOne, context),
        KeyBinding::new("3", TypeThree, context),
        KeyBinding::new("5", TypeFive, context),
    ]);
}

impl TreeView {
    fn move_rows(&mut self, delta: i64, cx: &mut Context<Self>) {
        if let Some(ready) = self.ready_mut() {
            ready.outline.move_by(delta);
            self.follow(rusty_structured::Step::Done, cx);
        }
    }

    fn page_rows(&self) -> i64 {
        i64::try_from(self.page.max(1)).unwrap_or(1)
    }

    fn go_to(&mut self, end: bool, cx: &mut Context<Self>) {
        if let Some(ready) = self.ready_mut() {
            let row = if end { usize::MAX } else { 0 };
            ready.outline.move_to(row);
            self.follow(rusty_structured::Step::Done, cx);
        }
    }

    fn fold(&mut self, open: bool, cx: &mut Context<Self>) {
        let Some(ready) = self.ready_mut() else {
            return;
        };
        let step = if open {
            ready.outline.right(&ready.tree)
        } else {
            ready.outline.left(&ready.tree);
            rusty_structured::Step::Done
        };
        self.follow(step, cx);
    }

    fn type_or_pass(&mut self, ch: char, cx: &mut Context<Self>) {
        if self.is_filtering() {
            self.type_char(ch, cx);
        } else {
            cx.propagate();
        }
    }

    fn expand_subtree(&mut self, cx: &mut Context<Self>) {
        if self.is_filtering() {
            self.type_char('*', cx);
        } else if let Some(ready) = self.ready_mut() {
            let step = ready.outline.expand_subtree(&ready.tree);
            self.follow(step, cx);
        }
    }

    fn collapse_all(&mut self, cx: &mut Context<Self>) {
        if self.is_filtering() {
            self.type_char('-', cx);
        } else if let Some(ready) = self.ready_mut() {
            ready.outline.collapse_all(&ready.tree);
            self.follow(rusty_structured::Step::Done, cx);
        }
    }

    fn back(&mut self, cx: &mut Context<Self>) {
        if self.is_filtering() {
            self.clear_filter(cx);
        } else if !self.stop_indexing(cx) {
            cx.propagate();
        }
    }

    fn copy_path(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = self.path.clone() {
            cx.write_to_clipboard(ClipboardItem::new_string(path.to_string()));
            self.set_note(format!("Copied {path}"), cx);
        }
    }

    fn type_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if !matches!(self.state, TreeState::Ready(_)) {
            return;
        }
        if let Some(ch) = typed_char(&event.keystroke) {
            self.type_char(ch, cx);
            cx.stop_propagation();
        }
    }

    pub(super) fn moves(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &RowUp, _, cx| view.move_rows(-1, cx)))
            .on_action(cx.listener(|view, _: &RowDown, _, cx| view.move_rows(1, cx)))
            .on_action(cx.listener(|view, _: &PageUp, _, cx| {
                view.move_rows(-view.page_rows(), cx);
            }))
            .on_action(cx.listener(|view, _: &PageDown, _, cx| {
                view.move_rows(view.page_rows(), cx);
            }))
            .on_action(cx.listener(|view, _: &GoStart, _, cx| view.go_to(false, cx)))
            .on_action(cx.listener(|view, _: &GoEnd, _, cx| view.go_to(true, cx)))
            .on_action(cx.listener(|view, _: &ScrollLeft, _, cx| view.fold(false, cx)))
            .on_action(cx.listener(|view, _: &ScrollRight, _, cx| view.fold(true, cx)))
    }

    pub(super) fn commands(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &ExpandSubtree, _, cx| view.expand_subtree(cx)))
            .on_action(cx.listener(|view, _: &CollapseAll, _, cx| view.collapse_all(cx)))
            .on_action(cx.listener(|view, _: &CopyPath, _, cx| view.copy_path(cx)))
            .on_action(cx.listener(|view, _: &FilterBack, _, cx| view.filter_back(cx)))
            .on_action(cx.listener(|view, _: &Back, _, cx| view.back(cx)))
            .on_action(cx.listener(|view, _: &TypeN, _, cx| view.type_or_pass('n', cx)))
            .on_action(cx.listener(|view, _: &TypeP, _, cx| view.type_or_pass('p', cx)))
            .on_action(cx.listener(|view, _: &TypeOne, _, cx| view.type_or_pass('1', cx)))
            .on_action(cx.listener(|view, _: &TypeThree, _, cx| view.type_or_pass('3', cx)))
            .on_action(cx.listener(|view, _: &TypeFive, _, cx| view.type_or_pass('5', cx)))
            .on_key_down(cx.listener(|view, event, _, cx| view.type_key(event, cx)))
    }
}
