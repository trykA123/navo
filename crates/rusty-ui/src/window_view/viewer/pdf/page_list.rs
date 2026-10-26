use std::ops::Range;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{ObjectFit, Role, StyledImage, WeakEntity, img};

use super::doc::Doc;
use super::sheets::list_frame;
use super::{LeftTab, PdfView};
use crate::format;
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant, Tab, Tabs, VirtualList};
use crate::tokens::Theme;
use crate::window_view::viewer::{ListClose, ListDown, ListGo, ListUp};

const ROW: f32 = 104.;
const THUMB: f32 = 64.;

fn badge(t: &Theme, hits: usize) -> Div {
    div()
        .px(t.spacing.sm)
        .rounded(t.radii.full)
        .bg(t.marks.fills[1])
        .text_size(t.typography.tiny)
        .text_color(t.colors.text)
        .font_family(t.typography.numbers_font.clone())
        .font_features(t.typography.numbers_font_features.clone())
        .child(format::count(hits as u64, "hit", "hits"))
}

fn thumb_frame(t: &Theme, doc: &Doc, page: u32, is_current: bool) -> Div {
    let size = doc.pages.size(page);
    let thumb = doc.thumbs.get(page).map(|shot| shot.image.clone());
    let edge = if is_current {
        t.colors.accent
    } else {
        t.colors.line
    };
    div()
        .flex_none()
        .w(px(THUMB))
        .h(px((THUMB * size.height / size.width.max(1.)).min(ROW - 8.)))
        .bg(t.surfaces.tile)
        .border_1()
        .border_color(edge)
        .when_some(thumb, |frame, image| {
            frame.child(img(image).size_full().object_fit(ObjectFit::Contain))
        })
}

fn page_focus_ring(t: &Theme, row: usize) -> impl IntoElement {
    div()
        .id(("pdf-page-focus-ring", row))
        .test_support()
        .absolute()
        .inset_0()
        .m(t.spacing.xxs)
        .rounded(t.radii.md)
        .shadow(t.shadows.focus.clone())
}

impl PdfView {
    pub(super) fn left_tabs(&self, _: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.weak_entity();
        let selected = match self.left_tab {
            LeftTab::Pages => 0,
            LeftTab::Outline => 1,
        };
        Tabs::new("pdf-left-tabs", [Tab::new("Pages"), Tab::new("Outline")])
            .selected(selected)
            .on_select(move |index, _, cx| {
                let tab = if *index == 0 {
                    LeftTab::Pages
                } else {
                    LeftTab::Outline
                };
                let _ = this.update(cx, |view, cx| view.pick_tab(tab, cx));
            })
    }

    pub(super) fn page_rows(&self) -> Vec<u32> {
        let Some(doc) = self.doc() else {
            return Vec::new();
        };
        if self.pages_with_hits && doc.hits.total() > 0 {
            doc.hits.page_list().collect()
        } else {
            (0..doc.count()).collect()
        }
    }

    pub(super) fn show_pages_row(&mut self) {
        let rows = self.page_rows();
        let Some(doc) = self.doc_mut() else {
            return;
        };
        let current = doc.current_page();
        let row = rows.iter().position(|page| *page >= current).unwrap_or(0);
        doc.lists.page_row = row;
        doc.lists.page_scroll.scroll_to(row);
    }

    fn toggle_with_hits(&mut self, cx: &mut Context<Self>) {
        self.pages_with_hits = !self.pages_with_hits;
        self.show_pages_row();
        cx.notify();
    }

    pub(super) fn move_page_row(&mut self, down: bool, cx: &mut Context<Self>) {
        let len = self.page_rows().len();
        let Some(doc) = self.doc_mut() else {
            return;
        };
        let row = doc.lists.page_row;
        doc.lists.page_row = if down {
            (row + 1).min(len.saturating_sub(1))
        } else {
            row.saturating_sub(1)
        };
        doc.lists.page_scroll.scroll_to(doc.lists.page_row);
        cx.notify();
    }

    pub(super) fn go_to_page_row(&mut self, row: usize, cx: &mut Context<Self>) {
        let Some(page) = self.page_rows().get(row).copied() else {
            return;
        };
        if let Some(doc) = self.doc_mut() {
            doc.lists.page_row = row;
        }
        self.go_to_page(page, cx);
    }

    fn page_row(
        &self,
        t: &Theme,
        (row, page, is_focused): (usize, u32, bool),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(doc) = self.doc() else {
            return div().into_any_element();
        };
        let hits = doc.hits.on(page).len();
        let is_selected = doc.lists.page_row == row;
        let hit_badge = (hits > 0).then(|| badge(t, hits));
        let theme = t.selected_content(is_selected);
        let t = &theme;
        let is_current = doc.current_page() == page;
        div()
            .when(t.is_contrast, |row| row.text_color(t.colors.text))
            .id(("pdf-page-row", row))
            .role(Role::ListItem)
            .aria_label(format!("Page {}", page + 1))
            .aria_selected(is_selected)
            .test_support()
            .relative()
            .flex()
            .items_center()
            .gap(t.spacing.lg)
            .w_full()
            .h(px(ROW))
            .px(t.spacing.lg)
            .rounded(t.radii.md)
            .when(is_selected, |row| row.bg(t.colors.selection))
            .when(is_selected && is_focused, |frame| {
                frame.child(page_focus_ring(t, row))
            })
            .on_click(cx.listener(move |view, _, _, cx| view.go_to_page_row(row, cx)))
            .child(thumb_frame(t, doc, page, is_current))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(t.spacing.xs)
                    .child(
                        div()
                            .when(is_current, |label| label.font_weight(FontWeight::SEMIBOLD))
                            .child(format!("Page {}", page + 1)),
                    )
                    .children(hit_badge),
            )
            .into_any_element()
    }

    fn render_page_rows(
        this: &WeakEntity<Self>,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<AnyElement> {
        let Some(view) = this.upgrade() else {
            return Vec::new();
        };
        let entity = view.entity_id();
        view.update(cx, |view, cx| {
            let t = crate::tokens::current(cx).clone();
            let rows = view.page_rows();
            let shown = rows.get(range.clone()).unwrap_or(&[]);
            let first = shown.first().copied().unwrap_or(0);
            let last = shown.last().map_or(0, |page| page + 1);
            let moved = view
                .doc()
                .is_some_and(|doc| doc.lists.thumbs_shown.replace((first, last)) != (first, last));
            if moved {
                window.on_next_frame(move |_, cx| cx.notify(entity));
            }
            let is_focused = view.pages_focus.is_focused(window);
            range
                .zip(shown.to_vec())
                .map(|(row, page)| view.page_row(&t, (row, page, is_focused), cx))
                .collect()
        })
    }

    fn pages_list(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let Some(doc) = self.doc() else {
            return div().into_any_element();
        };
        let rows = self.page_rows().len();
        let scroll = doc.lists.page_scroll.clone();
        let has_hits = doc.hits.total() > 0;
        let this = cx.weak_entity();
        let list = VirtualList::new("pdf-pages", rows, &scroll, move |range, window, cx| {
            Self::render_page_rows(&this, range, window, cx)
        })
        .row_height(px(ROW))
        .label("Pages");
        let variant = if self.pages_with_hits {
            ButtonVariant::Primary
        } else {
            ButtonVariant::Ghost
        };
        list_frame("pdf-page-list")
            .track_focus(&self.pages_focus)
            .on_action(cx.listener(|view, _: &ListUp, _, cx| view.move_page_row(false, cx)))
            .on_action(cx.listener(|view, _: &ListDown, _, cx| view.move_page_row(true, cx)))
            .on_action(cx.listener(|view, _: &ListGo, _, cx| {
                let row = view.doc().map_or(0, |doc| doc.lists.page_row);
                view.go_to_page_row(row, cx);
            }))
            .on_action(cx.listener(|view, _: &ListClose, window, cx| {
                view.close_sheet(crate::side_sheet::Side::Left, window, cx);
            }))
            .when(has_hits, |frame| {
                frame.child(
                    div().flex().px(t.spacing.lg).pb(t.spacing.sm).child(
                        Button::new("pdf-pages-with-hits", "Only pages with hits")
                            .variant(variant)
                            .on_click(cx.listener(|view, _, _, cx| view.toggle_with_hits(cx))),
                    ),
                )
            })
            .child(list)
            .into_any_element()
    }

    pub(super) fn left_body(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        match self.left_tab {
            LeftTab::Pages => self.pages_list(t, cx),
            LeftTab::Outline => self.outline_list(t, cx),
        }
    }
}
