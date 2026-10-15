use std::any::Any;

use gpui_kit::component::input::EditorState;
use gpui_kit::{AnyView, FocusHandle, Focusable};

use super::EditorView;
use crate::prelude::*;
use crate::window_view::viewer::FileText;
use crate::window_view::viewer::body::{BodyKeys, Host, ViewerBody};

pub(in crate::window_view::viewer) struct EditorBody(pub Entity<EditorView>);

impl EditorBody {
    pub(in crate::window_view::viewer) fn new(
        file: FileText,
        host: Host,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        Self(cx.new(|cx| EditorView::new(file, host, window, cx)))
    }

    pub(in crate::window_view::viewer) fn editor(&self, cx: &App) -> Entity<EditorState> {
        self.0.read(cx).editor.clone()
    }
}

fn find_in_editor(state: &mut EditorState, backward: bool, cx: &mut Context<EditorState>) {
    let session = state.search_session();
    if session.query.is_empty() {
        state.open_search(false, cx);
        return;
    }
    let (query, case_insensitive) = (session.query.clone(), session.case_insensitive);
    state.set_search_query(query, case_insensitive, cx);
    let _ = if backward {
        state.previous_search_match(cx)
    } else {
        state.next_search_match(cx)
    };
}

impl ViewerBody for EditorBody {
    fn view(&self) -> AnyView {
        self.0.clone().into()
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.0.read(cx).editor.read(cx).focus_handle(cx)
    }

    fn keys(&self, cx: &App) -> BodyKeys {
        let mut keys = vec![("Ctrl+F", "Find"), ("F3", "Next")];
        if self.is_editing(cx) {
            keys.push(("Ctrl+S", "Save"));
        }
        BodyKeys::Normal(keys)
    }

    fn find_again(&self, backward: bool, _: &mut Window, cx: &mut App) {
        let editor = self.editor(cx);
        editor.update(cx, |state, cx| find_in_editor(state, backward, cx));
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
