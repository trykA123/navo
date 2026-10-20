use gpui_kit::Point;
use rusty_l10n::t;
use rusty_pdf::protocol::LinkTarget;

use super::PdfView;
use super::layout::Anchor;
use super::links::{is_opened, link_at, scheme_of};
use crate::prelude::*;

const CLICK_SLOP: f32 = 4.;
const TOP_MARGIN: f32 = 24.;

impl PdfView {
    pub(super) fn click(&mut self, from: Point<f32>, to: Point<f32>, cx: &mut Context<Self>) {
        if (from.x - to.x).abs() > CLICK_SLOP || (from.y - to.y).abs() > CLICK_SLOP {
            return;
        }
        let target = self
            .doc()
            .and_then(|doc| link_at(doc, from))
            .map(|link| link.target.clone());
        if let Some(target) = target {
            self.follow(target, cx);
        }
    }

    pub(super) fn follow(&mut self, target: LinkTarget, cx: &mut Context<Self>) {
        match target {
            LinkTarget::Page { page, top } => self.jump(page, top, cx),
            LinkTarget::Uri(uri) => self.open_link(&uri, cx),
            LinkTarget::Unsupported => {}
        }
    }

    fn jump(&mut self, page: u32, top: Option<f32>, cx: &mut Context<Self>) {
        let Some(doc) = self.doc_mut() else {
            return;
        };
        let scale = doc.scale();
        let here = doc.pages.anchor(doc.camera.scroll.y, scale);
        doc.trail.leave(here);
        doc.selection = None;
        let page = page.min(doc.count().saturating_sub(1));
        match top {
            Some(top) => {
                doc.camera.scroll.y = doc.pages.top(page, scale) + top * scale - TOP_MARGIN;
                doc.camera.clamp(&doc.pages);
            }
            None => doc.camera.go_to(&doc.pages, page),
        }
        cx.notify();
    }

    fn open_link(&mut self, uri: &str, cx: &mut Context<Self>) {
        match scheme_of(uri) {
            Some(scheme) if is_opened(&scheme) => (self.open_uri)(uri),
            other => {
                let scheme = other.unwrap_or_else(|| t!("pdf-link-scheme-unknown"));
                self.set_note(t!("pdf-link-not-opened", scheme = scheme), cx);
            }
        }
    }

    fn restore(&mut self, spot: Anchor, cx: &mut Context<Self>) {
        if let Some(doc) = self.doc_mut() {
            doc.camera.scroll.y = doc.pages.place(spot, doc.scale());
            doc.camera.clamp(&doc.pages);
        }
        cx.notify();
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "Alt+Left and Alt+Right bind it in P37 U2")
    )]
    pub(in crate::window_view::viewer) fn go_back(&mut self, cx: &mut Context<Self>) {
        let spot = self.doc_mut().and_then(|doc| {
            let here = doc.pages.anchor(doc.camera.scroll.y, doc.scale());
            doc.trail.step_back(here)
        });
        if let Some(spot) = spot {
            self.restore(spot, cx);
        }
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "Alt+Left and Alt+Right bind it in P37 U2")
    )]
    pub(in crate::window_view::viewer) fn go_forward(&mut self, cx: &mut Context<Self>) {
        let spot = self.doc_mut().and_then(|doc| {
            let here = doc.pages.anchor(doc.camera.scroll.y, doc.scale());
            doc.trail.step_forward(here)
        });
        if let Some(spot) = spot {
            self.restore(spot, cx);
        }
    }
}
