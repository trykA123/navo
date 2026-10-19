use std::collections::BTreeMap;

use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::{MouseButton, MouseDownEvent, Stateful};

use super::PdfView;
use super::doc::Doc;
use crate::format;
use crate::prelude::*;
use crate::tokens::Theme;

const SLOTS: f32 = 400.;
const RAIL_WIDTH: f32 = 10.;
const MAP_HEIGHT: f32 = 20.;
const MAX_PAGE_TICKS: u32 = 240;

fn fraction(doc: &Doc, page: u32, y: f32) -> f32 {
    let total = doc.pages.height(1.).max(1.);
    ((doc.pages.top(page, 1.) + y * doc.pages.size(page).height) / total).clamp(0., 1.)
}

fn hit_slots(doc: &Doc) -> BTreeMap<u32, bool> {
    let current = doc.hits.current();
    let mut slots: BTreeMap<u32, bool> = BTreeMap::new();
    for hit in doc.hits.all() {
        let slot = (fraction(doc, hit.page, hit.y) * SLOTS) as u32;
        let is_current = current.is_some_and(|current| current == *hit);
        *slots.entry(slot).or_default() |= is_current;
    }
    slots
}

fn tick(t: &Theme, is_current: bool) -> Div {
    let color = if is_current {
        t.colors.accent
    } else {
        t.marks.ticks[1]
    };
    div().absolute().bg(color).rounded(t.radii.xs)
}

pub(super) fn hit_rail(t: &Theme, doc: &Doc) -> Option<ObservedElement<Stateful<Div>>> {
    if doc.hits.total() == 0 {
        return None;
    }
    let ticks = hit_slots(doc).into_iter().map(|(slot, is_current)| {
        let tall = if is_current { 4. } else { 2. };
        tick(t, is_current)
            .top(relative(slot as f32 / SLOTS))
            .left(px(2.))
            .right(px(2.))
            .h(px(tall))
    });
    let label = format!(
        "Hit rail: {}",
        format::count(doc.hits.total() as u64, "hit", "hits")
    );
    Some(
        div()
            .id("pdf-rail")
            .aria_label(label)
            .test_support()
            .absolute()
            .top(px(4.))
            .bottom(px(4.))
            .right(px(2.))
            .w(px(RAIL_WIDTH))
            .children(ticks),
    )
}

fn page_ticks(t: &Theme, doc: &Doc) -> Vec<Div> {
    let count = doc.count().max(1);
    let every = count.div_ceil(MAX_PAGE_TICKS).max(1);
    (0..count)
        .step_by(every as usize)
        .map(|page| {
            div()
                .absolute()
                .left(relative(fraction(doc, page, 0.)))
                .bottom_0()
                .h(px(MAP_HEIGHT / 3.))
                .w(px(1.))
                .bg(t.colors.line)
        })
        .collect()
}

fn view_window(t: &Theme, doc: &Doc) -> Div {
    let total = doc.pages.height(doc.scale()).max(1.);
    let from = (doc.camera.scroll.y / total).clamp(0., 1.);
    let span = (doc.camera.viewport.height / total).clamp(0.005, 1.);
    div()
        .absolute()
        .top_0()
        .bottom_0()
        .left(relative(from))
        .w(relative(span))
        .rounded(t.radii.xs)
        .border_1()
        .border_color(t.colors.accent)
        .bg(t.colors.accent.opacity(0.15))
}

impl PdfView {
    fn jump_on_map(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        let measured = self.measured.get();
        let x = f32::from(event.position.x) - measured.origin.x;
        let share = (x / measured.size.width.max(1.)).clamp(0., 1.);
        let Some(page) = self
            .doc()
            .map(|doc| doc.pages.page_at(share * doc.pages.height(1.), 1.))
        else {
            return;
        };
        self.go_to_page(page, cx);
    }

    pub(super) fn page_map(
        &self,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> Option<ObservedElement<Stateful<Div>>> {
        let doc = self.doc().filter(|_| self.look.page_map)?;
        let hits = hit_slots(doc).into_iter().map(|(slot, is_current)| {
            tick(t, is_current)
                .left(relative(slot as f32 / SLOTS))
                .top(px(2.))
                .h(px(MAP_HEIGHT / 2.))
                .w(px(2.))
        });
        Some(
            div()
                .id("pdf-page-map")
                .aria_label(format!(
                    "Page map: page {} of {}",
                    doc.current_page() + 1,
                    doc.count()
                ))
                .test_support()
                .relative()
                .flex_none()
                .h(px(MAP_HEIGHT))
                .my(t.spacing.xs)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|view, event: &MouseDownEvent, _, cx| view.jump_on_map(event, cx)),
                )
                .children(page_ticks(t, doc))
                .child(view_window(t, doc))
                .children(hits),
        )
    }
}
