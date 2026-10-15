use std::any::Any;

use gpui_kit::{AnyView, FocusHandle, Focusable};

use super::EditorView;
use crate::prelude::*;
use crate::window_view::viewer::FileText;
use crate::window_view::viewer::body::{BodyAct, BodyKeys, Host, Key, ViewerBody};
use crate::window_view::viewer::search::bar::{FIND_KEYS, LIST_KEYS, MARK_KEYS};

pub(in crate::window_view::viewer) struct EditorFile {
    pub file: FileText,
    pub language: &'static str,
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
        if view.find.bar.is_some() {
            return BodyKeys::Modal(FIND_KEYS.to_vec());
        }
        let mut keys: Vec<Key> = vec![("Ctrl+F", "Find"), ("F3", "Next")];
        if view.editing {
            keys.push(("Ctrl+S", "Save"));
        }
        keys.extend(MARK_KEYS);
        if !view.editing {
            keys.push(("Alt+L", "Line ends"));
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
