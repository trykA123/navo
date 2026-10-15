use gpui_kit::{FocusHandle, Focusable};

use super::{FileHooks, OpenAs, PagedKind, PagedView, Viewer, attach_editor};
use crate::prelude::*;
use crate::window_view::FerriteWindow;

const HEX_WARNING: &str = "Unsaved changes. Ctrl+S saves; Alt+3 again discards them.";

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
}

fn attach_paged(
    files: &FileHooks,
    viewer: &mut Viewer,
    kind: PagedKind,
    cx: &mut Context<FerriteWindow>,
) -> Result<(), String> {
    let paged = (files.paged)(&viewer.location)?;
    let title = viewer.title.clone();
    viewer.editable = paged.editable;
    viewer.paged = Some(cx.new(|cx| PagedView::new(title, paged, kind, cx)));
    viewer.editor = None;
    viewer._changes = None;
    viewer.dirty = false;
    viewer.leaving = false;
    viewer.to_hex = false;
    viewer.overwrite = false;
    viewer.message = None;
    Ok(())
}

impl FerriteWindow {
    pub(super) fn open_first(
        &mut self,
        viewer: &mut Viewer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        match (self.files.pick)(&viewer.location)? {
            OpenAs::Text => {
                let file = (self.files.read)(&viewer.location)?;
                attach_editor(viewer, file, window, cx);
                Ok(())
            }
            OpenAs::Hex => attach_paged(&self.files, viewer, PagedKind::Hex, cx),
            OpenAs::PagedText => attach_paged(&self.files, viewer, PagedKind::Text, cx),
        }
    }

    pub(super) fn show_hex(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(viewer) = &mut self.viewer else {
            return;
        };
        if let Some(paged) = viewer.paged.clone() {
            paged.update(cx, |paged, cx| paged.set_kind(PagedKind::Hex, cx));
            window.focus(&paged.read(cx).focus_handle(cx), cx);
            return;
        }
        if viewer.dirty && !viewer.to_hex {
            viewer.to_hex = true;
            viewer.message = Some(HEX_WARNING.into());
            cx.notify();
            return;
        }
        if let Err(message) = attach_paged(&self.files, viewer, PagedKind::Hex, cx) {
            viewer.message = Some(message.into());
        }
        let focus = viewer.focus_target(cx);
        window.focus(&focus, cx);
        cx.notify();
    }

    pub(super) fn show_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(viewer) = &mut self.viewer else {
            return;
        };
        let Some(paged) = viewer.paged.clone() else {
            return;
        };
        if !viewer.editable {
            paged.update(cx, |paged, cx| paged.set_kind(PagedKind::Text, cx));
            window.focus(&paged.read(cx).focus_handle(cx), cx);
            return;
        }
        match (self.files.read)(&viewer.location) {
            Ok(file) => {
                viewer.paged = None;
                attach_editor(viewer, file, window, cx);
            }
            Err(message) => {
                paged.update(cx, |paged, cx| paged.set_note(message, cx));
                return;
            }
        }
        let focus = viewer.focus_target(cx);
        window.focus(&focus, cx);
        cx.notify();
    }
}
