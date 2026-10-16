use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{KeyDownEvent, MouseButton, MouseDownEvent, Role, Stateful, WeakEntity};
use rusty_fs::archive::ArchiveListing;

use super::rows::{self, Look};
use super::{ArchiveView, CONTEXT};
use crate::file_list::row::filter_bar;
use crate::format;
use crate::prelude::*;
use crate::primitives::{RowSemantics, VirtualList};
use crate::tokens::{self, Theme};
use crate::window_view::viewer::{
    CloseViewer, FindOnEnter, GoEnd, GoStart, PageDown, PageUp, RowDown, RowUp, ViewerBody as _,
};

const CHROME_ROWS: f32 = 6.;

fn small(t: &Theme) -> Div {
    div()
        .flex_none()
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
}

fn status(t: &Theme, text: SharedString) -> impl IntoElement {
    small(t)
        .id("archive-status")
        .role(Role::Status)
        .aria_label(text.clone())
        .test_support()
        .child(text)
}

struct Rows {
    listing: Rc<ArchiveListing>,
    shown: Rc<Vec<usize>>,
    cursor: usize,
    has_packed: bool,
    this: WeakEntity<ArchiveView>,
}

impl Rows {
    fn render(&self, row: usize, t: &Theme) -> Option<AnyElement> {
        let entry = self.listing.entries.get(*self.shown.get(row)?)?;
        let look = Look {
            theme: t,
            entry,
            row,
            is_cursor: row == self.cursor,
            has_packed: self.has_packed,
        };
        let this = self.this.clone();
        let click = move |event: &MouseDownEvent, window: &mut Window, cx: &mut App| {
            let is_double = event.click_count >= 2;
            let _ = this.update(cx, |view, cx| view.click((row, is_double), window, cx));
        };
        Some(
            rows::cells(&look)
                .on_mouse_down(MouseButton::Left, click)
                .into_any_element(),
        )
    }

    fn describe(&self, row: usize) -> RowSemantics {
        let entry = self
            .shown
            .get(row)
            .and_then(|index| self.listing.entries.get(*index));
        RowSemantics {
            label: entry.map(|entry| rows::spoken(entry).into()),
            is_selected: None,
            is_active_descendant: row == self.cursor,
        }
    }
}

impl ArchiveView {
    fn status_text(&self) -> Option<SharedString> {
        if let Some(reading) = &self.reading {
            return Some(format!("Reading {}…", reading.name).into());
        }
        self.note.clone().or_else(|| self.truncated_note())
    }

    fn truncated_note(&self) -> Option<SharedString> {
        let shown = self.listing.entries.len() as u64;
        self.listing.truncated.then(|| {
            let count = format::count(shown, "entry", "entries");
            format!("Showing the first {count}.").into()
        })
    }

    fn header(&self, t: &Theme) -> Div {
        let count = format::count(self.listing.entries.len() as u64, "item", "items");
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.md)
            .px(t.spacing.xl)
            .h(t.spacing.path_height)
            .border_b_1()
            .border_color(t.colors.line)
            .bg(t.colors.rail)
            .child(
                div()
                    .id("viewer-title")
                    .test_support()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.title.clone()),
            )
            .child(small(t).child(self.listing.format.label()))
            .child(small(t).child(count))
            .child(small(t).child(format::size(self.totals.size)))
            .child(div().flex_1())
            .children(self.status_text().map(|text| status(t, text)))
    }

    fn column_titles(t: &Theme, has_packed: bool) -> Div {
        rows::columns(t)
            .flex_none()
            .h(t.spacing.row_height)
            .border_b_1()
            .border_color(t.colors.line)
            .text_size(t.typography.small)
            .text_color(t.colors.faint)
            .child(
                div()
                    .flex_1()
                    .pl(t.spacing.icon + t.spacing.md)
                    .child("Name"),
            )
            .child(rows::right(t.spacing.size_column, "Size"))
            .when(has_packed, |d| {
                d.child(rows::right(t.spacing.size_column, "Packed"))
            })
            .child(rows::right(t.spacing.date_column + t.spacing.xl, "Date"))
    }

    fn empty(&self, t: &Theme) -> impl IntoElement {
        let text = if self.listing.entries.is_empty() {
            "This archive is empty."
        } else {
            "No entries match the filter."
        };
        div()
            .id("archive-empty")
            .role(Role::Status)
            .aria_label(text)
            .test_support()
            .flex_1()
            .p(t.spacing.xxl)
            .text_color(t.colors.muted)
            .child(text)
    }

    fn list(&self, has_packed: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = Rc::new(Rows {
            listing: self.listing.clone(),
            shown: self.shown.clone(),
            cursor: self.cursor,
            has_packed,
            this: cx.weak_entity(),
        });
        let describe = rows.clone();
        let label = format!("{} entries", self.title);
        VirtualList::new(
            "archive-list",
            self.shown.len(),
            &self.scroll,
            move |range, _, cx| {
                let t = tokens::current(cx);
                range.filter_map(|row| rows.render(row, t)).collect()
            },
        )
        .label(label)
        .describe_rows(move |row| describe.describe(row))
    }

    fn actions(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &RowUp, _, cx| view.move_by(-1, cx)))
            .on_action(cx.listener(|view, _: &RowDown, _, cx| view.move_by(1, cx)))
            .on_action(cx.listener(|view, _: &PageUp, _, cx| view.move_by(-view.page(), cx)))
            .on_action(cx.listener(|view, _: &PageDown, _, cx| view.move_by(view.page(), cx)))
            .on_action(cx.listener(|view, _: &GoStart, _, cx| view.move_to(0, cx)))
            .on_action(cx.listener(|view, _: &GoEnd, _, cx| view.move_to(usize::MAX, cx)))
            .on_action(cx.listener(|view, _: &FindOnEnter, window, cx| {
                view.open_cursor(window, cx);
            }))
            .on_action(cx.listener(|view, _: &CloseViewer, window, cx| view.back(window, cx)))
            .on_key_down(cx.listener(|view, event: &KeyDownEvent, _, cx| {
                view.type_key(event, cx);
            }))
    }

    fn listing_view(&mut self, t: &Theme, window: &Window, cx: &mut Context<Self>) -> Div {
        let rows = (window.viewport_size().height / t.spacing.row_height - CHROME_ROWS).floor();
        self.page_rows = if rows.is_finite() && rows >= 1. {
            rows as usize
        } else {
            1
        };
        let has_packed = self.totals.has_packed;
        let body = div().flex().flex_col().flex_1().min_h_0();
        let body = if self.shown.is_empty() {
            body.child(self.empty(t))
        } else {
            body.child(Self::column_titles(t, has_packed))
                .child(self.list(has_packed, cx))
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(self.header(t))
            .child(body)
            .when(!self.filter.is_empty(), |d| {
                d.child(filter_bar(t, self.filter.clone().into(), self.shown.len()))
            })
    }
}

impl Render for ArchiveView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let root = div()
            .id("archive")
            .key_context(CONTEXT)
            .track_focus(&self.focus);
        let root = Self::actions(root, cx)
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .size_full()
            .bg(t.colors.surface);
        match &self.nested {
            Some(nested) => root.child(nested.view()),
            None => root.child(self.listing_view(&t, window, cx)),
        }
    }
}
