use gpui_kit::{Action, ClipboardItem, KeyBinding, KeyDownEvent, Stateful};
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
    let context = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("*", ExpandSubtree, context),
        KeyBinding::new("-", CollapseAll, context),
        KeyBinding::new("ctrl-c", CopyValue, context),
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
        KeyBinding::new("alt-right", OpenSubtree, context),
        KeyBinding::new("alt-left", CloseSubtree, context),
        KeyBinding::new("ctrl-shift-right", OpenAll, context),
        KeyBinding::new("ctrl-shift-left", CloseAll, context),
        KeyBinding::new("ctrl-1", FoldDepth1, context),
        KeyBinding::new("ctrl-2", FoldDepth2, context),
        KeyBinding::new("ctrl-3", FoldDepth3, context),
        KeyBinding::new("ctrl-4", FoldDepth4, context),
        KeyBinding::new("ctrl-5", FoldDepth5, context),
        KeyBinding::new("ctrl-6", FoldDepth6, context),
        KeyBinding::new("ctrl-7", FoldDepth7, context),
        KeyBinding::new("ctrl-8", FoldDepth8, context),
        KeyBinding::new("ctrl-9", FoldDepth9, context),
    ]);
}

pub(super) fn menu_entries() -> Vec<MenuEntry> {
    let depths: Vec<MenuEntry> = vec![
        MenuEntry::Run("1", Box::new(FoldDepth1)),
        MenuEntry::Run("2", Box::new(FoldDepth2)),
        MenuEntry::Run("3", Box::new(FoldDepth3)),
        MenuEntry::Run("4", Box::new(FoldDepth4)),
        MenuEntry::Run("5", Box::new(FoldDepth5)),
        MenuEntry::Run("6", Box::new(FoldDepth6)),
        MenuEntry::Run("7", Box::new(FoldDepth7)),
        MenuEntry::Run("8", Box::new(FoldDepth8)),
        MenuEntry::Run("9", Box::new(FoldDepth9)),
    ];
    vec![
        MenuEntry::Run("Copy value", Box::new(CopyValue)),
        MenuEntry::Run("Copy key", Box::new(CopyKey)),
        MenuEntry::Run("Copy path", Box::new(CopyPath)),
        MenuEntry::Break,
        MenuEntry::Run("Fold branch", Box::new(CloseSubtree)),
        MenuEntry::Run("Fold all", Box::new(CloseAll)),
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
            .on_action(cx.listener(|view, _: &CopyValue, _, cx| view.copy_value(cx)))
            .on_action(cx.listener(|view, _: &CopyKey, _, cx| view.copy_key(cx)))
            .on_action(cx.listener(|view, _: &FilterBack, _, cx| view.filter_back(cx)))
            .on_action(cx.listener(|view, _: &Back, _, cx| view.back(cx)))
            .on_action(cx.listener(|view, _: &TypeN, _, cx| view.type_or_pass('n', cx)))
            .on_action(cx.listener(|view, _: &TypeP, _, cx| view.type_or_pass('p', cx)))
            .on_action(cx.listener(|view, _: &TypeOne, _, cx| view.type_or_pass('1', cx)))
            .on_action(cx.listener(|view, _: &TypeThree, _, cx| view.type_or_pass('3', cx)))
            .on_action(cx.listener(|view, _: &TypeFive, _, cx| view.type_or_pass('5', cx)))
            .on_key_down(cx.listener(|view, event, _, cx| view.type_key(event, cx)))
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
