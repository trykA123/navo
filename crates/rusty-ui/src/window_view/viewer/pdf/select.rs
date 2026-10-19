use gpui_kit::{ClipboardItem, MouseButton, MouseDownEvent, MouseMoveEvent, Point, point};

use super::PdfView;
use super::doc::{Doc, Selection};
use super::text::{char_at, copied};
use crate::format;
use crate::prelude::*;

const SELECT_FIRST: &str = "Drag over the text on a page to select it first.";
const NO_TEXT: &str = "This page has no text to copy.";

pub(super) fn char_under(doc: &Doc, at: Point<f32>) -> Option<(u32, u32)> {
    let scale = doc.scale();
    let y = doc.camera.scroll.y + at.y;
    let page = doc.pages.page_at(y, scale);
    let size = doc.pages.size(page);
    let x = (at.x - doc.camera.page_left(&doc.pages, page)) / scale;
    let y = (y - doc.pages.top(page, scale)) / scale;
    let inside = (0. ..=size.width).contains(&x) && (0. ..=size.height).contains(&y);
    let text = doc.boxes.get(page).filter(|_| inside)?;
    char_at(text, x, y).map(|at| (page, at))
}

impl PdfView {
    pub(super) fn local(&self, position: Point<Pixels>) -> Point<f32> {
        let origin = self.measured.get().origin;
        point(
            f32::from(position.x) - origin.x,
            f32::from(position.y) - origin.y,
        )
    }

    pub(super) fn press(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let at = self.local(event.position);
        self.focus_pages(window, cx);
        let Some(doc) = self.doc_mut() else {
            return;
        };
        doc.selection = char_under(doc, at).map(|(page, at)| Selection {
            page,
            from: at,
            to: at,
        });
        doc.is_dragging = doc.selection.is_some();
        cx.notify();
    }

    pub(super) fn drag(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let at = self.local(event.position);
        let Some(doc) = self.doc_mut().filter(|doc| doc.is_dragging) else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            doc.is_dragging = false;
            return;
        }
        let under = char_under(doc, at);
        if let (Some(selection), Some((page, to))) = (doc.selection.as_mut(), under)
            && selection.page == page
            && selection.to != to
        {
            selection.to = to;
            cx.notify();
        }
    }

    pub(super) fn release(&mut self) {
        if let Some(doc) = self.doc_mut() {
            doc.is_dragging = false;
        }
    }

    pub(super) fn copy_selection(&mut self, cx: &mut Context<Self>) {
        let Some(doc) = self.doc() else {
            return;
        };
        let Some(selection) = doc.selection else {
            return self.set_note(SELECT_FIRST, cx);
        };
        let Some(text) = doc.boxes.get(selection.page) else {
            return self.set_note(NO_TEXT, cx);
        };
        let picked = copied(text, selection.from, selection.to);
        let count = picked.chars().count() as u64;
        cx.write_to_clipboard(ClipboardItem::new_string(picked));
        self.set_note(
            format!("Copied {}", format::count(count, "character", "characters")),
            cx,
        );
    }
}
