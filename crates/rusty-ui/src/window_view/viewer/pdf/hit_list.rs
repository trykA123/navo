use std::ops::Range;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{FontWeight, HighlightStyle, Role, StyledText, WeakEntity};

use super::PdfView;
use super::search::{Hit, snippet};
use super::sheets::list_frame;
use crate::format;
use crate::prelude::*;
use crate::primitives::VirtualList;
use crate::tokens::Theme;
use crate::window_view::viewer::{ListClose, ListDown, ListGo, ListUp};

const ROW: f32 = 56.;
const PAGE_COLUMN: f32 = 52.;

struct HitRow {
    row: usize,
    hit: Hit,
    is_first: bool,
}

fn hit_style(t: &Theme) -> HighlightStyle {
    HighlightStyle {
        font_weight: Some(FontWeight::BOLD),
        background_color: Some(t.marks.fills[1]),
        ..HighlightStyle::default()
    }
}

fn page_column(t: &Theme, page: String) -> Div {
    small(t)
        .flex_none()
        .w(px(PAGE_COLUMN))
        .font_family(t.typography.numbers_font.clone())
        .child(page)
}

fn small(t: &Theme) -> Div {
    div()
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
}

impl PdfView {
    pub(super) fn indexing_label(&self) -> Option<SharedString> {
        let doc = self.doc().filter(|_| !self.query.is_empty())?;
        if doc.index.is_complete() {
            return doc
                .index
                .is_full()
                .then(|| "Search stopped early: this PDF has too much text to index.".into());
        }
        let at = (doc.index.indexed() + 1).min(doc.count());
        Some(
            format!(
                "Indexing page {} of {}",
                format::bytes(u64::from(at)),
                format::bytes(u64::from(doc.count()))
            )
            .into(),
        )
    }

    pub(super) fn hits_title(&self, t: &Theme) -> impl IntoElement {
        let summary = self.doc().map(|doc| {
            let pages = doc.hits.page_count() as u64;
            let total = doc.hits.total() as u64;
            let hits = if doc.hits.is_capped() {
                format!("{}+ hits", format::bytes(total))
            } else {
                format::count(total, "hit", "hits")
            };
            let on = format::count(pages, "page", "pages");
            let so_far = if doc.index.is_complete() {
                ""
            } else {
                " so far"
            };
            format!("{hits} on {on}{so_far}")
        });
        div()
            .flex()
            .items_baseline()
            .gap(t.spacing.md)
            .child(div().font_weight(FontWeight::SEMIBOLD).child("All hits"))
            .children(summary.map(|summary| {
                small(t)
                    .id("pdf-hits-summary")
                    .aria_label(summary.clone())
                    .test_support()
                    .child(summary)
            }))
    }

    pub(super) fn move_hit_row(&mut self, down: bool, cx: &mut Context<Self>) {
        let Some(doc) = self.doc_mut() else {
            return;
        };
        let last = doc.hits.total().saturating_sub(1);
        let row = doc.selected_hit_row();
        let row = if down {
            (row + 1).min(last)
        } else {
            row.saturating_sub(1)
        };
        doc.lists.hit_key = doc.hits.get(row).map(|hit| hit.key());
        doc.lists.hit_scroll.scroll_to(row);
        cx.notify();
    }

    fn hit_row(&self, t: &Theme, shown_row: HitRow, cx: &mut Context<Self>) -> AnyElement {
        let HitRow { row, hit, is_first } = shown_row;
        let Some(doc) = self.doc() else {
            return div().into_any_element();
        };
        let (text, marked) = doc
            .index
            .get(hit.page)
            .map(|words| snippet(&words.chars, &hit))
            .map_or_else(Default::default, |shown| {
                let marked = shown.before.len()..shown.before.len() + shown.hit.len();
                (
                    format!("{}{}{}", shown.before, shown.hit, shown.after),
                    marked,
                )
            });
        let is_selected = doc.lists.hit_key == Some(hit.key());
        let page = if is_first {
            format!("p. {}", hit.page + 1)
        } else {
            String::new()
        };
        div()
            .id(("pdf-hit-row", row))
            .role(Role::ListItem)
            .aria_label(format!("Page {}: {text}", hit.page + 1))
            .aria_selected(is_selected)
            .test_support()
            .flex()
            .gap(t.spacing.md)
            .h(px(ROW))
            .px(t.spacing.lg)
            .py(t.spacing.sm)
            .overflow_hidden()
            .when(is_first, |row| row.border_t_1().border_color(t.colors.line))
            .when(is_selected, |row| row.bg(t.colors.selection))
            .on_click(cx.listener(move |view, _, _, cx| view.go_to_hit(row, cx)))
            .child(page_column(t, page))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .line_clamp(2)
                    .text_size(t.typography.small)
                    .child(StyledText::new(text).with_highlights([(marked, hit_style(t))])),
            )
            .into_any_element()
    }

    fn render_hit_rows(
        this: &WeakEntity<Self>,
        range: Range<usize>,
        cx: &mut App,
    ) -> Vec<AnyElement> {
        let Some(view) = this.upgrade() else {
            return Vec::new();
        };
        view.update(cx, |view, cx| {
            let t = crate::tokens::current(cx).clone();
            let Some(doc) = view.doc() else {
                return Vec::new();
            };
            let all = doc.hits.all();
            let mut previous = range
                .start
                .checked_sub(1)
                .and_then(|at| all.get(at))
                .map(|hit| hit.page);
            let shown: Vec<Hit> = all.get(range.clone()).unwrap_or(&[]).to_vec();
            range
                .zip(shown)
                .map(|(row, hit)| {
                    let is_first = previous != Some(hit.page);
                    previous = Some(hit.page);
                    view.hit_row(&t, HitRow { row, hit, is_first }, cx)
                })
                .collect()
        })
    }

    pub(super) fn hits_body(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let Some(doc) = self.doc() else {
            return div().into_any_element();
        };
        let this = cx.weak_entity();
        let list = VirtualList::new(
            "pdf-hits",
            doc.hits.total(),
            &doc.lists.hit_scroll,
            move |range, _, cx| Self::render_hit_rows(&this, range, cx),
        )
        .row_height(px(ROW))
        .label("All hits");
        let empty = (doc.hits.total() == 0).then(|| match self.query.is_empty() {
            true => "Type in Find to list every hit here.",
            false => "No hits yet.",
        });
        list_frame("pdf-hit-list")
            .track_focus(&self.hits_focus)
            .on_action(cx.listener(|view, _: &ListUp, _, cx| view.move_hit_row(false, cx)))
            .on_action(cx.listener(|view, _: &ListDown, _, cx| view.move_hit_row(true, cx)))
            .on_action(cx.listener(|view, _: &ListGo, _, cx| {
                let row = view.doc().map_or(0, super::doc::Doc::selected_hit_row);
                view.go_to_hit(row, cx);
            }))
            .on_action(cx.listener(|view, _: &ListClose, window, cx| {
                view.close_sheet(crate::side_sheet::Side::Right, window, cx);
            }))
            .children(self.indexing_label().map(|label| {
                small(t)
                    .id("pdf-hits-indexing")
                    .role(Role::Status)
                    .aria_label(label.clone())
                    .test_support()
                    .px(t.spacing.lg)
                    .pb(t.spacing.sm)
                    .child(label)
            }))
            .children(empty.map(|note| small(t).p(t.spacing.lg).child(note)))
            .child(list)
            .into_any_element()
    }
}
