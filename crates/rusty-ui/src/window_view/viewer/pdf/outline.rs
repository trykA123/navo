use std::ops::Range;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{Role, WeakEntity};
use rusty_l10n::t;
use rusty_pdf::protocol::{LinkTarget, OutlineItem};

use super::PdfView;
use super::links::middle_cut;
use super::sheets::list_frame;
use crate::format;
use crate::prelude::*;
use crate::primitives::{Tooltip, VirtualList};
use crate::tokens::Theme;
use crate::window_view::viewer::{ListClose, ListDown, ListGo, ListUp};

const ROW: f32 = 32.;
const INDENT: f32 = 12.;
const MAX_STEPS: usize = 6;
const TITLE_CHARS: usize = 40;
const PAGE_COLUMN: f32 = 44.;
const SCROLLBAR: f32 = 20.;

struct ChapterRow {
    row: usize,
    item: OutlineItem,
    is_current: bool,
}

fn title_cell(t: &Theme, row: usize, item: &OutlineItem, is_current: bool) -> impl IntoElement {
    let steps = usize::from(item.level).min(MAX_STEPS);
    let color = match (is_current, item.target != LinkTarget::Unsupported) {
        (true, _) => t.colors.accent,
        (false, true) => t.colors.text,
        (false, false) => t.colors.muted,
    };
    let full = SharedString::from(item.title.clone());
    div()
        .id(("pdf-outline-title", row))
        .test_support()
        .flex_1()
        .min_w_0()
        .ml(px(INDENT * steps as f32))
        .overflow_hidden()
        .whitespace_nowrap()
        .text_color(color)
        .when(is_current, |title| title.font_weight(FontWeight::SEMIBOLD))
        .tooltip(move |window, cx| Tooltip::new(full.clone()).build(window, cx))
        .child(middle_cut(
            &item.title,
            TITLE_CHARS.saturating_sub(steps * 2),
        ))
}

fn page_cell(t: &Theme, row: usize, page: String) -> impl IntoElement {
    div()
        .id(("pdf-outline-page", row))
        .aria_label(page.clone())
        .test_support()
        .flex_none()
        .w(px(PAGE_COLUMN))
        .text_right()
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .font_family(t.typography.numbers_font.clone())
        .child(page)
}

impl PdfView {
    pub(super) fn move_outline_row(&mut self, down: bool, cx: &mut Context<Self>) {
        let Some(doc) = self.doc_mut() else {
            return;
        };
        let last = doc.chapters.items().len().saturating_sub(1);
        let row = doc.lists.outline_row;
        doc.lists.outline_row = if down {
            (row + 1).min(last)
        } else {
            row.saturating_sub(1)
        };
        doc.lists.outline_scroll.scroll_to(doc.lists.outline_row);
        cx.notify();
    }

    pub(super) fn go_to_outline_row(&mut self, row: usize, cx: &mut Context<Self>) {
        let Some(doc) = self.doc_mut() else {
            return;
        };
        let Some(target) = doc
            .chapters
            .items()
            .get(row)
            .map(|item| item.target.clone())
        else {
            return;
        };
        doc.lists.outline_row = row;
        self.follow(target, cx);
    }

    fn chapter_row(&self, t: &Theme, shown: ChapterRow, cx: &mut Context<Self>) -> AnyElement {
        let ChapterRow {
            row,
            item,
            is_current,
        } = shown;
        let is_selected = self.doc().is_some_and(|doc| doc.lists.outline_row == row);
        let is_followed = item.target != LinkTarget::Unsupported;
        let page = match item.target {
            LinkTarget::Page { page, .. } => format::bytes(u64::from(page) + 1),
            _ => String::new(),
        };
        let full = SharedString::from(item.title.clone());
        div()
            .id(("pdf-outline-row", row))
            .role(Role::ListItem)
            .aria_label(full)
            .aria_selected(is_current)
            .test_support()
            .flex()
            .items_center()
            .gap(t.spacing.md)
            .w_full()
            .h(px(ROW))
            .pl(t.spacing.lg)
            .pr(px(SCROLLBAR))
            .rounded(t.radii.md)
            .when(is_selected, |row| row.bg(t.colors.selection))
            .when(is_followed, |row| row.cursor_pointer())
            .on_click(cx.listener(move |view, _, _, cx| view.go_to_outline_row(row, cx)))
            .child(title_cell(t, row, &item, is_current))
            .child(page_cell(t, row, page))
            .into_any_element()
    }

    fn render_chapter_rows(
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
            let items = doc.chapters.items();
            let current = doc.chapters.current(doc.reading_spot());
            let shown: Vec<OutlineItem> = items.get(range.clone()).unwrap_or(&[]).to_vec();
            range
                .zip(shown)
                .map(|(row, item)| {
                    let is_current = current == Some(row);
                    let shown = ChapterRow {
                        row,
                        item,
                        is_current,
                    };
                    view.chapter_row(&t, shown, cx)
                })
                .collect()
        })
    }

    pub(super) fn outline_list(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let Some(doc) = self.doc() else {
            return div().into_any_element();
        };
        let this = cx.weak_entity();
        let list = VirtualList::new(
            "pdf-outline-items",
            doc.chapters.items().len(),
            &doc.lists.outline_scroll,
            move |range, _, cx| Self::render_chapter_rows(&this, range, cx),
        )
        .row_height(px(ROW))
        .label("Outline");
        let note = if doc.trouble.outline_lost {
            Some(SharedString::from(t!("pdf-outline-unreadable")))
        } else {
            (doc.chapters.is_read() && doc.chapters.items().is_empty())
                .then(|| SharedString::from(t!("pdf-outline-empty")))
        };
        list_frame("pdf-outline")
            .track_focus(&self.pages_focus)
            .on_action(cx.listener(|view, _: &ListUp, _, cx| view.move_outline_row(false, cx)))
            .on_action(cx.listener(|view, _: &ListDown, _, cx| view.move_outline_row(true, cx)))
            .on_action(cx.listener(|view, _: &ListGo, _, cx| {
                let row = view.doc().map_or(0, |doc| doc.lists.outline_row);
                view.go_to_outline_row(row, cx);
            }))
            .on_action(cx.listener(|view, _: &ListClose, window, cx| {
                view.close_sheet(crate::side_sheet::Side::Left, window, cx);
            }))
            .children(note.map(|note| {
                div()
                    .id("pdf-outline-note")
                    .role(Role::Status)
                    .aria_label(note.clone())
                    .test_support()
                    .p(t.spacing.xl)
                    .text_color(t.colors.muted)
                    .child(note)
            }))
            .child(list)
            .into_any_element()
    }
}
