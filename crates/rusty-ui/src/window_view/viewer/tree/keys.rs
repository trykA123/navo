use gpui_kit::{Action, ClipboardItem, KeyDownEvent, Stateful};
use rusty_structured::{Outline, Step, Tree};

use super::{TreeState, TreeView};
use crate::file_list::typed_char;
use crate::prelude::*;
use crate::window_view::viewer::body::MenuEntry;
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
        CopyValue,
        CopyKey,
        FilterBack,
        TypeN,
        TypeP,
        TypeOne,
        TypeThree,
        TypeFive,
        OpenSubtree,
        CloseSubtree,
        OpenAll,
        CloseAll,
        FoldDepth1,
        FoldDepth2,
        FoldDepth3,
        FoldDepth4,
        FoldDepth5,
        FoldDepth6,
        FoldDepth7,
        FoldDepth8,
        FoldDepth9,
    ]
);

pub(in crate::window_view::viewer) fn bind_keys(cx: &mut App) {
    crate::commands::bind_namespace(cx, "viewer_tree");
}

pub(super) fn menu_entries() -> Vec<MenuEntry> {
    let depths: Vec<MenuEntry> = vec![
        MenuEntry::Run("1".into(), Box::new(FoldDepth1), false),
        MenuEntry::Run("2".into(), Box::new(FoldDepth2), false),
        MenuEntry::Run("3".into(), Box::new(FoldDepth3), false),
        MenuEntry::Run("4".into(), Box::new(FoldDepth4), false),
        MenuEntry::Run("5".into(), Box::new(FoldDepth5), false),
        MenuEntry::Run("6".into(), Box::new(FoldDepth6), false),
        MenuEntry::Run("7".into(), Box::new(FoldDepth7), false),
        MenuEntry::Run("8".into(), Box::new(FoldDepth8), false),
        MenuEntry::Run("9".into(), Box::new(FoldDepth9), false),
    ];
    vec![
        MenuEntry::Run("Copy value".into(), Box::new(CopyValue), false),
        MenuEntry::Run("Copy key".into(), Box::new(CopyKey), false),
        MenuEntry::Run("Copy path".into(), Box::new(CopyPath), false),
        MenuEntry::Break,
        MenuEntry::Run("Fold branch".into(), Box::new(CloseSubtree), false),
        MenuEntry::Run("Fold all".into(), Box::new(CloseAll), false),
        MenuEntry::Submenu("Depth", depths),
    ]
}

fn reshape_on<A: Action>(
    root: Stateful<Div>,
    change: fn(&mut Outline, &Tree) -> Step,
    cx: &mut Context<TreeView>,
) -> Stateful<Div> {
    root.on_action(cx.listener(move |view, _: &A, _, cx| view.reshape(change, cx)))
}

fn depth_on<A: Action>(
    root: Stateful<Div>,
    depth: u16,
    cx: &mut Context<TreeView>,
) -> Stateful<Div> {
    root.on_action(cx.listener(move |view, _: &A, _, cx| {
        view.reshape(
            move |outline, tree| {
                outline.fold_to_depth(tree, depth);
                Step::Done
            },
            cx,
        );
    }))
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

    fn type_or_pass(&mut self, ch: char, window: &Window, cx: &mut Context<Self>) {
        if self.is_filtering() && !self.is_typing_in_bar(window, cx) {
            self.type_char(ch, cx);
        } else {
            cx.propagate();
        }
    }

    fn expand_subtree(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.is_typing_in_bar(window, cx) {
            cx.propagate();
        } else if self.is_filtering() {
            self.type_char('*', cx);
        } else if let Some(ready) = self.ready_mut() {
            let step = ready.outline.expand_subtree(&ready.tree);
            self.follow(step, cx);
        }
    }

    fn collapse_all(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.is_typing_in_bar(window, cx) {
            cx.propagate();
        } else if self.is_filtering() {
            self.type_char('-', cx);
        } else if let Some(ready) = self.ready_mut() {
            ready.outline.collapse_all(&ready.tree);
            self.follow(rusty_structured::Step::Done, cx);
        }
    }

    fn reshape(
        &mut self,
        change: impl FnOnce(&mut Outline, &Tree) -> Step,
        cx: &mut Context<Self>,
    ) {
        if self.is_filtering() {
            return;
        }
        if let Some(ready) = self.ready_mut() {
            let step = change(&mut ready.outline, &ready.tree);
            self.follow(step, cx);
        }
    }

    fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.close_bar(window, cx) {
            return;
        }
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

    fn type_key(&mut self, event: &KeyDownEvent, window: &Window, cx: &mut Context<Self>) {
        if !matches!(self.state, TreeState::Ready(_)) || self.is_typing_in_bar(window, cx) {
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
        root.on_action(cx.listener(|view, _: &ExpandSubtree, window, cx| {
            view.expand_subtree(window, cx);
        }))
        .on_action(cx.listener(|view, _: &CollapseAll, window, cx| {
            view.collapse_all(window, cx);
        }))
        .on_action(cx.listener(|view, _: &CopyPath, _, cx| view.copy_path(cx)))
        .on_action(cx.listener(|view, _: &CopyValue, _, cx| view.copy_value(cx)))
        .on_action(cx.listener(|view, _: &CopyKey, _, cx| view.copy_key(cx)))
        .on_action(cx.listener(|view, _: &FilterBack, window, cx| {
            view.filter_back(window, cx);
        }))
        .on_action(cx.listener(|view, _: &Back, window, cx| view.back(window, cx)))
        .on_action(cx.listener(|view, _: &TypeN, window, cx| {
            view.type_or_pass('n', window, cx);
        }))
        .on_action(cx.listener(|view, _: &TypeP, window, cx| {
            view.type_or_pass('p', window, cx);
        }))
        .on_action(cx.listener(|view, _: &TypeOne, window, cx| {
            view.type_or_pass('1', window, cx);
        }))
        .on_action(cx.listener(|view, _: &TypeThree, window, cx| {
            view.type_or_pass('3', window, cx);
        }))
        .on_action(cx.listener(|view, _: &TypeFive, window, cx| {
            view.type_or_pass('5', window, cx);
        }))
        .on_key_down(cx.listener(|view, event, window, cx| view.type_key(event, window, cx)))
    }

    pub(super) fn folds(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        let root = reshape_on::<OpenSubtree>(root, Outline::expand_subtree, cx);
        let root = reshape_on::<CloseSubtree>(
            root,
            |outline, tree| {
                outline.close_subtree(tree);
                Step::Done
            },
            cx,
        );
        let root = reshape_on::<OpenAll>(
            root,
            |outline, tree| {
                outline.expand_all(tree);
                Step::Done
            },
            cx,
        );
        let root = reshape_on::<CloseAll>(
            root,
            |outline, tree| {
                outline.collapse_all(tree);
                Step::Done
            },
            cx,
        );
        let root = depth_on::<FoldDepth1>(root, 1, cx);
        let root = depth_on::<FoldDepth2>(root, 2, cx);
        let root = depth_on::<FoldDepth3>(root, 3, cx);
        let root = depth_on::<FoldDepth4>(root, 4, cx);
        let root = depth_on::<FoldDepth5>(root, 5, cx);
        let root = depth_on::<FoldDepth6>(root, 6, cx);
        let root = depth_on::<FoldDepth7>(root, 7, cx);
        let root = depth_on::<FoldDepth8>(root, 8, cx);
        depth_on::<FoldDepth9>(root, 9, cx)
    }
}
