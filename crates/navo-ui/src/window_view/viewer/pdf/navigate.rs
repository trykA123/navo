use gpui_kit::{Focusable, Point, point};

use super::PdfView;
use super::layout::{ACTUAL, STEP, Zoom};
use crate::prelude::*;

const LINE: f32 = 48.;

fn parse_page(text: &str, count: u32) -> Result<u32, String> {
    let trimmed = text.trim();
    match trimmed.parse::<u32>() {
        Ok(page) if (1..=count).contains(&page) => Ok(page - 1),
        _ => Err(format!("No page {trimmed}. Pages run from 1 to {count}.")),
    }
}

impl PdfView {
    fn with_camera(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut super::doc::Doc)) {
        if let Some(doc) = self.doc_mut() {
            change(doc);
            doc.camera.clamp(&doc.pages);
        }
        cx.notify();
    }

    pub(super) fn go_to_page(&mut self, page: u32, cx: &mut Context<Self>) {
        self.with_camera(cx, |doc| doc.camera.go_to(&doc.pages, page));
    }

    pub(super) fn step_page(&mut self, forward: bool, cx: &mut Context<Self>) {
        self.with_camera(cx, |doc| {
            let current = doc.current_page();
            let page = if forward {
                current + 1
            } else {
                current.saturating_sub(1)
            };
            doc.camera.go_to(&doc.pages, page);
        });
    }

    pub(super) fn go_to_end(&mut self, cx: &mut Context<Self>) {
        self.with_camera(cx, |doc| {
            let last = doc.count().saturating_sub(1);
            doc.camera.go_to(&doc.pages, last);
        });
    }

    pub(super) fn scroll_by(&mut self, delta: Point<f32>, cx: &mut Context<Self>) {
        self.with_camera(cx, |doc| {
            let scroll = doc.camera.scroll;
            doc.camera.scroll = point(scroll.x + delta.x, scroll.y + delta.y);
        });
    }

    pub(super) fn scroll_lines(&mut self, across: f32, down: f32, cx: &mut Context<Self>) {
        self.scroll_by(point(across * LINE, down * LINE), cx);
    }

    pub(super) fn set_zoom(&mut self, zoom: Zoom, cx: &mut Context<Self>) {
        self.with_camera(cx, |doc| {
            let middle = doc.camera.middle();
            doc.camera.set_zoom(&doc.pages, zoom, middle);
            doc.forget_drawing();
        });
    }

    pub(super) fn actual_size(&mut self, cx: &mut Context<Self>) {
        self.set_zoom(Zoom::Scale(ACTUAL), cx);
    }

    pub(super) fn zoom_step(
        &mut self,
        inward: bool,
        pointer: Option<Point<f32>>,
        cx: &mut Context<Self>,
    ) {
        let factor = if inward { STEP } else { 1. / STEP };
        self.zoom_by(factor, pointer, cx);
    }

    pub(super) fn zoom_by(
        &mut self,
        factor: f32,
        pointer: Option<Point<f32>>,
        cx: &mut Context<Self>,
    ) {
        self.with_camera(cx, |doc| {
            let pointer = pointer.unwrap_or_else(|| doc.camera.middle());
            doc.camera.zoom_by(&doc.pages, factor, pointer);
            doc.forget_drawing();
        });
    }

    pub(super) fn open_page_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(page) = self.doc().map(|doc| doc.current_page() + 1) else {
            return;
        };
        self.inputs.page.update(cx, |input, cx| {
            input.set_value(page.to_string(), window, cx);
            input.select_all(window, cx);
        });
        let focus = self.inputs.page.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        cx.notify();
    }

    pub(super) fn sync_page_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(page) = self.doc().map(|doc| (doc.current_page() + 1).to_string()) else {
            return;
        };
        let input = self.inputs.page.read(cx);
        if input.focus_handle(cx).is_focused(window) || input.value() == page.as_str() {
            return;
        }
        self.inputs
            .page
            .update(cx, |input, cx| input.set_value(page, window, cx));
    }

    pub(super) fn submit_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(count) = self.doc().map(super::doc::Doc::count) else {
            return;
        };
        let typed = self.inputs.page.read(cx).value().to_string();
        match parse_page(&typed, count) {
            Ok(page) => {
                self.note = None;
                self.go_to_page(page, cx);
                self.focus_pages(window, cx);
            }
            Err(message) => self.set_note(message, cx),
        }
    }

    pub(super) fn leave_page_box(&mut self, window: &Window, cx: &mut Context<Self>) -> bool {
        let focus = self.inputs.page.read(cx).focus_handle(cx);
        if !focus.is_focused(window) {
            return false;
        }
        self.note = None;
        cx.notify();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::parse_page;

    #[test]
    fn page_numbers_are_one_based_and_bounded() {
        assert_eq!(parse_page(" 3 ", 10), Ok(2));
        assert!(parse_page("0", 10).is_err());
        assert!(parse_page("11", 10).is_err());
        assert!(parse_page("x", 10).is_err());
    }
}
