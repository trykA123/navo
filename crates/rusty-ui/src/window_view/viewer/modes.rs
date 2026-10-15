use gpui_kit::{FocusHandle, Focusable};

use super::open::{Loaded, load, start};
use super::{Kind, Paged, PagedKind, PagedView, Shape, Slot, Viewer, attach_editor};
use crate::prelude::*;
use crate::window_view::FerriteWindow;

impl Viewer {
    pub(super) fn focus_target(&self, cx: &App) -> FocusHandle {
        if let Some(paged) = &self.paged {
            return paged.read(cx).focus_handle(cx);
        }
        match &self.editor {
            Some(editor) => editor.read(cx).focus_handle(cx),
            None => self.focus.clone(),
        }
    }

    fn clear_body(&mut self) {
        self.editor = None;
        self.paged = None;
        self._changes = None;
        self.editing = false;
        self.dirty = false;
        self.leaving = false;
        self.switching = None;
        self.overwrite = false;
        self.message = None;
    }

    fn switch_warning(&mut self, slot: Slot) -> bool {
        if !self.dirty || self.switching == Some(slot) {
            return false;
        }
        self.switching = Some(slot);
        self.message = Some(
            format!(
                "Unsaved changes. Ctrl+S saves; {} again discards them.",
                slot.alt_key()
            )
            .into(),
        );
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Pick {
    pub slot: Slot,
    pub edit: bool,
}

pub(super) struct Shown {
    pub kind: Kind,
    pub loaded: Result<Loaded, String>,
    pub edit: bool,
}

fn attach_paged(
    viewer: &mut Viewer,
    paged: Paged,
    kind: PagedKind,
    cx: &mut Context<FerriteWindow>,
) {
    let title = viewer.title.clone();
    viewer.clear_body();
    viewer.paged = Some(cx.new(|cx| PagedView::new(title, paged, kind, cx)));
}

impl FerriteWindow {
    pub(super) fn show_loaded(
        &mut self,
        shown: Shown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(viewer) = &mut self.viewer else {
            return;
        };
        viewer.opening = None;
        viewer.kind = Some(shown.kind);
        match shown.loaded {
            Ok(Loaded::Text(file)) => {
                viewer.clear_body();
                attach_editor(viewer, file, window, cx);
                if shown.edit {
                    self.start_editing(cx);
                }
            }
            Ok(Loaded::Paged(paged, kind)) => attach_paged(viewer, paged, kind, cx),
            Err(message) => viewer.message = Some(message.into()),
        }
        if let Some(viewer) = &self.viewer {
            window.focus(&viewer.focus_target(cx), cx);
        }
        cx.notify();
    }

    pub(super) fn start_editing(&mut self, cx: &mut Context<Self>) {
        let Some(viewer) = self.viewer.as_mut().filter(|viewer| viewer.can_edit) else {
            return;
        };
        let Some(editor) = viewer.editor.clone() else {
            return;
        };
        viewer.editing = true;
        editor.update(cx, |state, cx| state.set_readonly(false, cx));
        cx.notify();
    }

    pub(super) fn show_slot(&mut self, pick: Pick, window: &mut Window, cx: &mut Context<Self>) {
        let files = self.files.clone();
        let Some(viewer) = &mut self.viewer else {
            return;
        };
        let Some(kind) = viewer.kind_in(pick.slot) else {
            return;
        };
        let shape = kind.shape(&viewer.sniff());
        let is_shown = viewer
            .kind
            .as_ref()
            .is_some_and(|shown| shown.id() == kind.id());
        if is_shown && (viewer.editor.is_some() || viewer.paged.is_some()) {
            return;
        }
        if shape != Shape::Editor && viewer.switch_warning(pick.slot) {
            cx.notify();
            return;
        }
        if let (Shape::Paged(paged_kind), Some(paged)) = (shape, viewer.paged.clone()) {
            viewer.kind = Some(kind);
            paged.update(cx, |paged, cx| paged.set_kind(paged_kind, cx));
            window.focus(&paged.read(cx).focus_handle(cx), cx);
            return;
        }
        let location = viewer.location.clone();
        viewer.clear_body();
        viewer.opening = Some(start(
            window,
            cx,
            move || load(&files, &location, shape),
            move |view: &mut Self, loaded, window, cx| {
                let shown = Shown {
                    kind,
                    loaded,
                    edit: pick.edit,
                };
                view.show_loaded(shown, window, cx);
            },
        ));
        window.focus(&viewer.focus, cx);
        cx.notify();
    }
}
