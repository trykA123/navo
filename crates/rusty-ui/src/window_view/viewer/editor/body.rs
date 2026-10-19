use std::any::Any;

use gpui_kit::component::input::{Copy, Cut, Paste, SelectAll};
use gpui_kit::{AnyView, FocusHandle, Focusable};

use super::{EditorView, SplitChoice};
use crate::prelude::*;
use crate::settings::Values;
use crate::window_view::viewer::FileText;
use crate::window_view::viewer::body::{
    BodyAct, BodyKeys, Carried, Host, Key, MenuEntry, ViewerBody,
};
use crate::window_view::viewer::menu::marking_entries;
use crate::window_view::viewer::parts::{PartSet, TEXT};
use crate::window_view::viewer::search::bar::{FIND_KEYS, GO_TO_KEYS, LIST_KEYS, MARK_KEYS};
use crate::window_view::viewer::{CopyPathLine, FindSelection, OpenGoTo, ShowHex, ToggleFollow};

pub(in crate::window_view::viewer) struct EditorFile {
    pub file: FileText,
    pub language: &'static str,
    pub split: Option<SplitChoice>,
}

pub(in crate::window_view::viewer) struct EditorBody(pub Entity<EditorView>);

impl EditorBody {
    pub(in crate::window_view::viewer) fn new(
        file: EditorFile,
        host: Host,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        Self(cx.new(|cx| EditorView::new(file, host, window, cx)))
    }

    #[cfg(test)]
    pub(in crate::window_view::viewer) fn editor(
        &self,
        cx: &App,
    ) -> Entity<gpui_kit::component::input::EditorState> {
        self.0.read(cx).editor.clone()
    }

    #[cfg(test)]
    pub(in crate::window_view::viewer) fn find_text(&self, cx: &App) -> Option<String> {
        self.0.read(cx).typed(cx)
    }

    #[cfg(test)]
    pub(in crate::window_view::viewer) fn preview_renders(&self, cx: &App) -> usize {
        self.0
            .read(cx)
            .split
            .as_ref()
            .map_or(0, |split| split.renders)
    }

    #[cfg(test)]
    pub(in crate::window_view::viewer) fn preview_text(&self, cx: &App) -> Option<String> {
        let split = self.0.read(cx).split.as_ref()?;
        Some(split.view().read(cx).rendered())
    }

    #[cfg(test)]
    pub(in crate::window_view::viewer) fn focused_side(
        &self,
        window: &Window,
        cx: &App,
    ) -> Option<&'static str> {
        let view = self.0.read(cx);
        if view.is_preview_focused(window, cx) {
            Some("preview")
        } else if self.focus_handle(cx).is_focused(window) {
            Some("source")
        } else {
            None
        }
    }
}

impl ViewerBody for EditorBody {
    fn view(&self) -> AnyView {
        self.0.clone().into()
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.0.read(cx).editor.read(cx).focus_handle(cx)
    }

    fn keys(&self, cx: &App) -> BodyKeys {
        let view = self.0.read(cx);
        if view.marking.find_all.is_some() {
            return BodyKeys::Modal(LIST_KEYS.to_vec());
        }
        if view.go_to.is_some() {
            return BodyKeys::Modal(GO_TO_KEYS.to_vec());
        }
        if view.find.bar.is_some() {
            return BodyKeys::Modal(FIND_KEYS.to_vec());
        }
        let mut keys: Vec<Key> = vec![("Ctrl+F", "Find"), ("F3", "Next")];
        if view.editing {
            keys.push(("Ctrl+S", "Save"));
        }
        if view.editing && view.has_split() {
            keys.push(("Ctrl+Shift+V", "Preview"));
        }
        if view.is_split() {
            keys.push(("F6", "Focus preview"));
        }
        keys.extend(MARK_KEYS);
        if !view.editing {
            keys.extend([("Alt+L", "Line ends"), ("Alt+F", "Follow")]);
        }
        BodyKeys::Normal(keys)
    }

    fn find_again(&self, backward: bool, window: &mut Window, cx: &mut App) {
        self.0
            .update(cx, |view, cx| view.find_again(backward, window, cx));
    }

    fn act(&self, act: BodyAct, _: &mut Window, cx: &mut App) {
        self.0.update(cx, |view, cx| view.act(act, cx));
    }

    fn go_to_line(&self, line: u64, window: &mut Window, cx: &mut App) {
        self.0
            .update(cx, |view, cx| view.go_to_line(line, window, cx));
    }

    fn carry(&self, cx: &App) -> Option<Carried> {
        let view = self.0.read(cx);
        Some(Carried {
            marks: view.marking.marks.set.clone(),
            bookmarks: view
                .persists
                .then(|| view.marking.carried_bookmarks())
                .flatten(),
        })
    }

    fn menu(&self, _: bool, cx: &mut App) -> Vec<MenuEntry> {
        let editing = self.0.read(cx).editing;
        let mut entries = Vec::new();
        if editing {
            entries.push(MenuEntry::Run("Cut", Box::new(Cut), false));
        }
        entries.push(MenuEntry::Run("Copy", Box::new(Copy), false));
        if editing {
            entries.push(MenuEntry::Run("Paste", Box::new(Paste), false));
        }
        entries.push(MenuEntry::Run("Select all", Box::new(SelectAll), false));
        entries.push(MenuEntry::Break);
        entries.push(MenuEntry::Run(
            "Find selection",
            Box::new(FindSelection),
            false,
        ));
        entries.push(MenuEntry::Run(
            "Copy path:line",
            Box::new(CopyPathLine),
            false,
        ));
        entries.push(MenuEntry::Break);
        entries.extend(marking_entries());
        entries.push(MenuEntry::Break);
        entries.push(MenuEntry::Run("Go to line", Box::new(OpenGoTo), false));
        if !editing {
            entries.push(MenuEntry::Run("Follow", Box::new(ToggleFollow), false));
        }
        entries.push(MenuEntry::Run("Open as Hex", Box::new(ShowHex), false));
        entries
    }

    fn parts(&self, _: &App) -> Option<&'static PartSet> {
        Some(&TEXT)
    }

    fn show_parts(&self, values: &Values, cx: &mut App) {
        self.0.update(cx, |view, cx| view.show_parts(values, cx));
    }

    fn menu_anchor(&self, cx: &App) -> Option<Point<Pixels>> {
        let state = self.0.read(cx).editor.read(cx);
        let cursor = state.cursor();
        let bounds = state.range_to_bounds(&(cursor..cursor))?;
        Some(bounds.bottom_left())
    }

    fn set_note(&self, note: SharedString, cx: &mut App) {
        self.0.update(cx, |view, cx| {
            view.note = Some(note);
            cx.notify();
        });
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn is_editing(&self, cx: &App) -> bool {
        self.0.read(cx).editing
    }

    fn can_start_editing(&self, cx: &App) -> bool {
        let view = self.0.read(cx);
        view.can_edit && !view.editing
    }

    fn start_editing(&self, cx: &mut App) -> bool {
        self.0.update(cx, |view, cx| view.start_editing(cx))
    }

    fn is_dirty(&self, cx: &App) -> bool {
        self.0.read(cx).dirty
    }

    fn revision(&self, cx: &App) -> u64 {
        self.0.read(cx).revision
    }

    fn save(&self, cx: &mut App) {
        self.0.update(cx, |view, cx| view.save(cx));
    }
}
