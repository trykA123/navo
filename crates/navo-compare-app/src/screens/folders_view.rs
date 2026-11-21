//! The Folders view of the compare tab: a summary bar and a tree of folders and files over the
//! `FolderTree` model. The cursor moves with the arrow keys; Enter opens a pair of files.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, KeyBinding, ParentElement, Pixels, ReadGlobal, Render,
    ScrollStrategy, Styled, UniformListScrollHandle, Window, canvas, div, px, uniform_list,
};
use sidewise_ui::browser::{CompareFilter, CompareRow};
use sidewise_ui::compare_diff::map::{ChangeMap, MapEvent};

use self::menu::{CopyLeftPath, ExpandAll, OpenStream, RevealLeft, ShowMenu, menu_actions};
use super::folders_head::{SideRoot, head_view};
use super::folders_model::{FolderTree, TreeLine, map_hunks};
use super::folders_row::{LineCtx, line_view, summary_bar};
use super::folders_text::Meta;
use crate::menu_host::{MenuSlot, menu_view};
use crate::theme::SidewiseTheme;

gpui_kit::actions!(folders, [CursorUp, CursorDown, StepIn, StepOut, Activate]);

/// Key context of the view root; the cursor keys bind inside it.
const CONTEXT: &str = "Folders";

/// What the view tells its parent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoldersEvent {
    /// Enter on a file line: its row id.
    OpenPair(u64),
    /// Open in Stream on a file line: its row id.
    OpenStream(u64),
}

/// Binds the cursor keys inside the `Folders` context; the shell calls this once at start.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", CursorUp, Some(CONTEXT)),
        KeyBinding::new("down", CursorDown, Some(CONTEXT)),
        KeyBinding::new("right", StepIn, Some(CONTEXT)),
        KeyBinding::new("left", StepOut, Some(CONTEXT)),
        KeyBinding::new("enter", Activate, Some(CONTEXT)),
        KeyBinding::new("shift-enter", OpenStream, Some(CONTEXT)),
        KeyBinding::new("ctrl-shift-c", CopyLeftPath, Some(CONTEXT)),
        KeyBinding::new("ctrl-shift-e", RevealLeft, Some(CONTEXT)),
        KeyBinding::new("shift-right", ExpandAll, Some(CONTEXT)),
        KeyBinding::new("shift-f10", ShowMenu, Some(CONTEXT)),
    ]);
}

/// The folder tree, the cursor and the scroll state of one Folders view.
pub struct FoldersView {
    tree: FolderTree,
    cursor: usize,
    cursor_path: Option<String>,
    fade: Option<(String, u64)>,
    opened: u64,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
    map: Entity<ChangeMap>,
    meta: Meta,
    roots: Option<[SideRoot; 2]>,
    pub(super) menu: MenuSlot,
    /// Where the cursor line was last drawn; `Shift+F10` anchors the menu to it.
    pub(super) cursor_bounds: Rc<Cell<Option<gpui_kit::Bounds<Pixels>>>>,
}

impl EventEmitter<FoldersEvent> for FoldersView {}

impl Focusable for FoldersView {
    /// The handle the view's key context tracks.
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl FoldersView {
    /// An empty view with the cursor on the first line once rows arrive.
    pub fn new(cx: &mut Context<Self>) -> Self {
        let map = cx.new(|_| ChangeMap::new(map_hunks(&[]), 0).with_width(px(14.)));
        cx.subscribe(&map, |this, _, event: &MapEvent, cx| {
            let MapEvent::Jump(row) = *event;
            this.jump_to(row as usize, cx);
        })
        .detach();
        Self {
            tree: FolderTree::new(Vec::new()),
            cursor: 0,
            cursor_path: None,
            fade: None,
            opened: 0,
            scroll: UniformListScrollHandle::new(),
            focus: cx.focus_handle(),
            map,
            meta: Meta::Size,
            roots: None,
            menu: None,
            cursor_bounds: Rc::default(),
        }
    }

    /// Sets the two folder roots the headers name, as given: the caller makes local paths
    /// absolute, and a remote address is not a path on this PC.
    pub fn with_roots(mut self, roots: [SideRoot; 2]) -> Self {
        self.roots = Some(roots);
        self
    }

    /// Takes the list column's width; only a change of meta mode redraws.
    pub(super) fn record_width(&mut self, width: Pixels, cx: &mut Context<Self>) {
        let meta = Meta::for_width(f32::from(width));
        if meta != self.meta {
            self.meta = meta;
            crate::frame_notify::notify_after_frame(cx);
        }
    }

    /// Replaces the rows; the cursor stays on its path while that line still shows.
    pub fn set_rows(&mut self, rows: Vec<CompareRow>, cx: &mut Context<Self>) {
        self.tree.set_rows(rows);
        self.sync_cursor(cx);
        cx.notify();
    }

    /// Opens the ancestors of `path` and puts the cursor on it; false when it is not shown.
    pub fn select_path(&mut self, path: &str, cx: &mut Context<Self>) -> bool {
        for (index, _) in path.match_indices('/') {
            self.tree.set_expanded(&path[..index], true);
        }
        self.sync_cursor(cx);
        let lines = self.tree.lines();
        let at = lines.iter().position(|line| line.path == path);
        at.map(|at| self.move_to(at, &lines, cx)).is_some()
    }

    /// How many lines the tree shows now; for tests.
    #[cfg(test)]
    pub(crate) fn line_count(&self) -> usize {
        self.tree.lines().len()
    }

    /// The path of the line under the cursor.
    pub fn selected_path(&self) -> Option<String> {
        self.cursor_path.clone()
    }

    /// Puts the cursor back on its path, or on the nearest line when the path is gone; the
    /// change map follows the new lines.
    fn sync_cursor(&mut self, cx: &mut Context<Self>) {
        let lines = self.tree.lines();
        let found = self
            .cursor_path
            .as_ref()
            .and_then(|path| lines.iter().position(|line| &line.path == path));
        self.cursor = found.unwrap_or(self.cursor.min(lines.len().saturating_sub(1)));
        self.cursor_path = lines.get(self.cursor).map(|line| line.path.clone());
        let (hunks, total) = (map_hunks(&lines), lines.len() as u32);
        self.map
            .update(cx, |map, cx| map.set_hunks(hunks, total, cx));
    }

    /// A jump from the change map: the cursor goes to line `row`, centred in the list.
    fn jump_to(&mut self, row: usize, cx: &mut Context<Self>) {
        let lines = self.tree.lines();
        if row < lines.len() {
            self.move_to(row, &lines, cx);
            self.scroll.scroll_to_item(row, ScrollStrategy::Center);
        }
    }

    /// Moves the cursor to line `at` of `lines` and scrolls it into view.
    fn move_to(&mut self, at: usize, lines: &[TreeLine], cx: &mut Context<Self>) {
        self.cursor = at;
        self.cursor_path = lines.get(at).map(|line| line.path.clone());
        self.scroll.scroll_to_item(at, ScrollStrategy::Nearest);
        cx.notify();
    }

    /// Opens or closes the folder `path`; an opened folder fades its subtree in.
    fn set_open(&mut self, path: &str, open: bool, cx: &mut Context<Self>) {
        if self.tree.set_expanded(path, open) && open {
            self.opened += 1;
            self.fade = Some((path.to_string(), self.opened));
        }
        self.sync_cursor(cx);
        cx.notify();
    }

    /// Sets the filter from a chip click.
    pub(super) fn set_filter(&mut self, filter: CompareFilter, cx: &mut Context<Self>) {
        self.tree.set_filter(filter);
        self.sync_cursor(cx);
        cx.notify();
    }

    /// A click on line `at`: the cursor goes there; nothing opens.
    pub(super) fn click_line(&mut self, at: usize, cx: &mut Context<Self>) {
        let lines = self.tree.lines();
        if lines.get(at).is_some() {
            self.move_to(at, &lines, cx);
        }
    }

    /// A double click on line `at`: select it, then open the folder or the file pair.
    pub(super) fn double_line(&mut self, at: usize, cx: &mut Context<Self>) {
        self.click_line(at, cx);
        self.activate(cx);
    }

    /// Toggles the folder on line `at` without moving the cursor; files are ignored.
    pub(super) fn toggle_at(&mut self, at: usize, cx: &mut Context<Self>) {
        let lines = self.tree.lines();
        if let Some(line) = lines.get(at).filter(|line| line.folder) {
            self.set_open(&line.path, !line.expanded, cx);
        }
    }

    /// The two folder roots, when the compare gave them.
    pub(super) fn roots(&self) -> Option<&[SideRoot; 2]> {
        self.roots.as_ref()
    }

    /// Opens the folder under the cursor and every folder below it.
    pub(super) fn expand_all(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = self.cursor_path.clone() {
            self.tree.expand_all_under(&path);
            self.sync_cursor(cx);
            cx.notify();
        }
    }

    /// The lines and the line under the cursor; `None` while the tree is empty.
    pub(super) fn current(&self) -> Option<(Vec<TreeLine>, TreeLine)> {
        let lines = self.tree.lines();
        let line = lines.get(self.cursor)?.clone();
        Some((lines, line))
    }
}

impl FoldersView {
    /// A zero-hit overlay on the list column that reports its width to `record_width`.
    fn width_probe(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.weak_entity();
        canvas(
            move |bounds, _, cx| {
                let _ = this.update(cx, |view, cx| view.record_width(bounds.size.width, cx));
            },
            |_, (), _, _| {},
        )
        .absolute()
        .inset_0()
    }
}

impl FoldersView {
    /// What every line needs to draw: the theme, the cursor, the fade, the filter and the meta mode.
    fn line_ctx(&self, cx: &mut Context<Self>) -> LineCtx {
        let theme = *SidewiseTheme::global(cx);
        let m = if theme.mode.is_dark() {
            theme.spec.dark
        } else {
            theme.spec.light
        };
        LineCtx {
            theme,
            m,
            cursor: self.cursor,
            fade: self.fade.clone(),
            filter: self.tree.filter(),
            view: cx.entity(),
            cursor_bounds: self.cursor_bounds.clone(),
            meta: self.meta,
            mono: sidewise_kit::tokens::current(cx)
                .typography
                .mono_font
                .clone(),
        }
    }
}

impl Render for FoldersView {
    /// The summary chips above a virtual list of tree lines, with the change map at the right.
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ctx = self.line_ctx(cx);
        let lines = Rc::new(self.tree.lines());
        let head = head_view(self.roots.as_ref(), &ctx);
        let bar = summary_bar(self.tree.totals(), &ctx);
        let map = self.map.clone();
        let list = uniform_list("folder-lines", lines.len(), move |range, _, cx| {
            let seen = range.start as u32..range.end as u32;
            map.update(cx, |map, cx| map.set_viewport(seen, cx));
            range
                .map(|index| line_view(&lines[index], index, &ctx, cx))
                .collect()
        })
        .track_scroll(&self.scroll)
        .flex_1();
        let root = div()
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &CursorUp, _, cx| this.step(-1, cx)))
            .on_action(cx.listener(|this, _: &CursorDown, _, cx| this.step(1, cx)))
            .on_action(cx.listener(|this, _: &StepIn, _, cx| this.step_in(cx)))
            .on_action(cx.listener(|this, _: &StepOut, _, cx| this.step_out(cx)))
            .on_action(cx.listener(|this, _: &Activate, _, cx| this.activate(cx)))
            .on_action(cx.listener(|this, _: &ExpandAll, _, cx| this.expand_all(cx)))
            .flex()
            .size_full()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .relative()
                    .children(head)
                    .child(bar)
                    .child(list)
                    .child(self.width_probe(cx)),
            )
            .child(self.map.clone())
            .children(menu_view(&self.menu));
        menu_actions(root, cx)
    }
}

#[path = "folders_keys.rs"]
mod keys;
#[cfg(test)]
#[path = "folders_layout_tests.rs"]
mod layout_tests;
#[path = "folders_menu.rs"]
mod menu;
#[cfg(test)]
#[path = "folders_menu_tests.rs"]
mod menu_tests;
#[cfg(test)]
#[path = "folders_mouse_tests.rs"]
mod mouse_tests;
#[cfg(test)]
#[path = "folders_view_tests.rs"]
mod tests;
