use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::{ClickEvent, HighlightStyle, Role, Stateful, StyledText, WeakEntity};
use rusty_automotive::Row;

use super::keys::CONTEXT;
use super::text::{Check, address, ascii, gap_row, hex_cells, item_name, status};
use super::{Focus, Index, MemoryView};
use crate::prelude::*;
use crate::primitives::VirtualList;
use crate::tokens::{self, Theme};
use crate::window_view::viewer::rich_header::{HeaderParts, header};
use crate::window_view::viewer::search::bar::{BarLook, bar_row};

const COLUMNS: &str = "00 01 02 03 04 05 06 07  08 09 0A 0B 0C 0D 0E 0F";
const EMPTY: [Option<u8>; 16] = [None; 16];

fn mono(t: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(t.spacing.xl)
        .px(t.spacing.xl)
        .whitespace_nowrap()
        .overflow_hidden()
        .font_family(t.typography.mono_font.clone())
}

fn select_row(
    view: WeakEntity<MemoryView>,
    row: u64,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) {
    move |_, window, cx| {
        let _ = view.update(cx, |view, cx| {
            window.focus(&view.focus, cx);
            view.focus_in = Focus::Content;
            view.move_cursor(row, cx);
        });
    }
}

impl MemoryView {
    fn address_width(&self) -> usize {
        if self.ready().is_some_and(|ready| ready.wide) {
            18
        } else {
            10
        }
    }

    fn data_row(&self, t: &Theme, index: u64, at: u64) -> ObservedElement<Stateful<Div>> {
        let wide = self.address_width() > 10;
        let bytes = self.bytes_at(at).unwrap_or(&EMPTY);
        let (hex, missing) = hex_cells(bytes);
        let faint = HighlightStyle {
            color: Some(t.colors.faint),
            ..HighlightStyle::default()
        };
        let highlights = missing
            .into_iter()
            .map(|range| (range, faint))
            .collect::<Vec<_>>();
        let text = ascii(bytes);
        let label = format!("{}  {hex}  {text}", address(at, wide));
        mono(t)
            .id(("memory-row", index as usize))
            .aria_label(label)
            .test_support()
            .h_full()
            .child(div().text_color(t.colors.muted).child(address(at, wide)))
            .child(StyledText::new(hex).with_highlights(highlights))
            .child(div().text_color(t.colors.muted).child(text))
    }

    fn gap_view(
        &self,
        t: &Theme,
        index: u64,
        range: &std::ops::Range<u64>,
    ) -> ObservedElement<Stateful<Div>> {
        let wide = self.address_width() > 10;
        let label = gap_row(range, wide);
        div()
            .id(("memory-gap", index as usize))
            .aria_label(label.clone())
            .test_support()
            .flex()
            .items_center()
            .justify_center()
            .h_full()
            .bg(t.colors.zebra)
            .border_y_1()
            .border_color(t.colors.line.opacity(0.6))
            .text_size(t.typography.small)
            .text_color(t.colors.muted)
            .child(label)
    }

    fn row_view(&self, t: &Theme, index: u64, view: &WeakEntity<Self>) -> AnyElement {
        let Some(row) = self.ready().and_then(|ready| ready.layout.row(index)) else {
            return div().into_any_element();
        };
        let row = match row {
            Row::Data { address } => self.data_row(t, index, address),
            Row::Gap { range } => self.gap_view(t, index, &range),
        };
        row.w_full()
            .when(index == self.cursor, |row| row.bg(t.colors.hover))
            .on_click(select_row(view.clone(), index))
            .into_any_element()
    }

    fn list(&self, cx: &mut Context<Self>) -> VirtualList {
        let view = cx.weak_entity();
        let count = usize::try_from(self.rows()).unwrap_or(usize::MAX);
        VirtualList::new("memory-list", count, &self.scroll, move |range, _, cx| {
            let Some(entity) = view.upgrade() else {
                return Vec::new();
            };
            let t = tokens::current(cx).clone();
            let this = entity.read(cx);
            range
                .map(|index| this.row_view(&t, index as u64, &view))
                .collect()
        })
        .label("Memory image")
    }

    fn column_head(&self, t: &Theme) -> Div {
        let width = self.address_width();
        mono(t)
            .flex_none()
            .h(t.spacing.row_height)
            .text_size(t.typography.small)
            .text_color(t.colors.muted)
            .bg(t.colors.rail)
            .border_b_1()
            .border_color(t.colors.line)
            .child(format!("{:<width$}", "Address"))
            .child(COLUMNS)
            .child("ASCII")
    }

    fn progress(&self) -> Option<SharedString> {
        if let Index::Running { scanned, .. } = &self.index {
            let done = (u128::from(*scanned) * 100 / u128::from(self.len.max(1))).min(100);
            return Some(format!("Reading records… {done}%").into());
        }
        self.note.clone()
    }

    fn header_parts(&self) -> HeaderParts {
        let detail = self.ready().map(|ready| {
            let at = match ready.layout.row(self.cursor) {
                Some(Row::Data { address: at }) => address(at, ready.wide),
                Some(Row::Gap { range }) => address(range.start, ready.wide),
                None => String::new(),
            };
            let name = item_name(&ready.items, self.selected);
            ("memory-cursor", format!("{at} · {name}").into())
        });
        HeaderParts {
            title: self.title.clone(),
            mode: self.format.name().into(),
            len: self.len,
            detail,
            status: self.progress().map(|text| ("memory-progress", text)),
        }
    }

    fn header_view(&self, t: &Theme) -> Div {
        let bar = self.go_to.as_ref().map(|go_to| {
            let look = BarLook {
                id: "memory-go-to",
                input_id: "memory-go-to-input",
                error_id: "memory-go-to-error",
                label: "Go to address",
                input: &go_to.input,
                is_find: false,
                match_case: None,
                count: None,
                error: go_to.error.clone(),
            };
            bar_row(t, look, |_, _, _| {})
        });
        header(t, self.header_parts()).children(bar)
    }

    fn status_line(&self, t: &Theme) -> impl IntoElement {
        let mut checks = match self.ready() {
            Some(ready) => status(&ready.image),
            None => Vec::new(),
        };
        if let Some(error) = &self.read_error {
            checks.push(Check {
                text: format!("rows could not be read: {error}"),
                is_bad: true,
            });
        }
        let label = checks
            .iter()
            .map(|check| check.text.as_str())
            .collect::<Vec<_>>()
            .join(" · ");
        let parts = checks
            .into_iter()
            .enumerate()
            .map(|(at, Check { text, is_bad })| {
                let color = if is_bad {
                    t.colors.warn
                } else {
                    t.colors.muted
                };
                div()
                    .flex()
                    .gap(t.spacing.sm)
                    .when(at > 0, |part| {
                        part.child(div().text_color(t.colors.faint).child("·"))
                    })
                    .child(div().text_color(color).child(text))
            });
        div()
            .id("memory-status")
            .role(Role::Status)
            .aria_label(label)
            .test_support()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.sm)
            .h(t.spacing.status_height)
            .px(t.spacing.xl)
            .border_t_1()
            .border_color(t.colors.line)
            .bg(t.colors.rail)
            .text_size(t.typography.small)
            .children(parts)
    }

    fn measure(&mut self, window: &Window, t: &Theme) {
        let rows = (window.viewport_size().height / t.spacing.row_height).floor();
        self.page = if rows.is_finite() && rows > 6. {
            rows as u64 - 6
        } else {
            1
        };
    }
}

impl Render for MemoryView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        self.measure(window, &t);
        let top = self.scroll.top_row(t.spacing.row_height) as u64;
        self.request_rows(top, cx);
        let rows = div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(self.column_head(&t))
            .child(self.list(cx))
            .child(self.drawers(&t, window, cx));
        let content = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(rows)
            .child(self.status_line(&t));
        let root = div()
            .id("memory-image")
            .key_context(CONTEXT)
            .track_focus(&self.focus);
        Self::keys(root, cx)
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .size_full()
            .bg(t.colors.surface)
            .text_size(t.typography.body)
            .child(self.header_view(&t))
            .child(content)
    }
}
