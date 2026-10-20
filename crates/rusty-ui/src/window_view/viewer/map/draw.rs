use gpui_kit::base::TestSupportExt;
use gpui_kit::{ClickEvent, Role, WeakEntity};

use super::keys::{CONTEXT, TABLE_CONTEXT};
use super::select::{Column, Head};
use super::text::{Check, flavor, heads, row_label, status, summary};
use super::{Focus, Index, MapView};
use crate::file_list::filter_bar;
use crate::format;
use crate::prelude::*;
use crate::primitives::{IconName, VirtualList, icon};
use crate::tokens::{self, Theme};
use crate::window_view::viewer::rich_header::{HeaderParts, header};
use crate::window_view::viewer::search::bar::{BarLook, bar_row};

const CHAR_WIDTH: f32 = 7.2;

fn chars(t: &Theme, count: usize) -> Pixels {
    px(count as f32 * CHAR_WIDTH) + t.spacing.md * 2.
}

fn cell(t: &Theme, head: &Head, text: String) -> Div {
    let base = div()
        .flex()
        .items_center()
        .h_full()
        .px(t.spacing.md)
        .overflow_hidden()
        .whitespace_nowrap();
    let base = if head.width == 0 {
        base.flex_1().min_w_0()
    } else {
        base.flex_none().w(chars(t, head.width))
    };
    match head.column {
        Column::Address => base.font_family(t.typography.mono_font.clone()),
        Column::Size | Column::Used | Column::Free | Column::Percent => base
            .justify_end()
            .font_family(t.typography.numbers_font.clone()),
        _ => base,
    }
    .child(div().min_w_0().truncate().child(text))
}

fn sort_arrow(descending: bool, color: Hsla) -> impl IntoElement {
    let glyph = if descending {
        IconName::ArrowDown
    } else {
        IconName::ArrowUp
    };
    icon(glyph, px(10.), color)
}

fn select_row(
    view: WeakEntity<MapView>,
    row: usize,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) {
    move |_, window, cx| {
        let _ = view.update(cx, |view, cx| {
            window.focus(&view.focus, cx);
            view.focus_in = Focus::Content;
            view.go_address = None;
            view.move_cursor(row, cx);
            view.open_inspector_if_asked(cx);
        });
    }
}

impl MapView {
    fn row_view(&self, t: &Theme, row: usize, view: &WeakEntity<Self>) -> AnyElement {
        let (Some(loaded), Some(at)) = (self.loaded(), self.rows.get(row)) else {
            return div().into_any_element();
        };
        let at = *at as usize;
        let table = self.table();
        let cells = super::text::cells(loaded, table, at);
        let heads = heads(loaded, table);
        div()
            .id(("map-row", row))
            .aria_label(row_label(loaded, table, at))
            .test_support()
            .flex()
            .w_full()
            .h_full()
            .cursor_pointer()
            .when(row % 2 == 1, |line| line.bg(t.colors.zebra))
            .when(row == self.cursor, |line| line.bg(t.colors.hover))
            .children(
                heads
                    .iter()
                    .zip(cells)
                    .map(|(head, text)| cell(t, head, text)),
            )
            .on_click(select_row(view.clone(), row))
            .into_any_element()
    }

    fn list(&self, cx: &mut Context<Self>) -> VirtualList {
        let view = cx.weak_entity();
        VirtualList::new(
            "map-list",
            self.rows.len(),
            &self.scroll,
            move |range, _, cx| {
                let Some(entity) = view.upgrade() else {
                    return Vec::new();
                };
                let t = tokens::current(cx).clone();
                let this = entity.read(cx);
                range.map(|row| this.row_view(&t, row, &view)).collect()
            },
        )
        .label(self.table().name())
    }

    fn column_head(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let order = self.query.order();
        let table = self.table();
        let heads = self
            .loaded()
            .map_or_else(|| table.heads().to_vec(), |loaded| heads(loaded, table));
        let cells = heads.into_iter().enumerate().map(|(at, head)| {
            let column = head.column;
            let sorted = (order.column == column).then_some(order.descending);
            let arrow = sorted.map(|descending| sort_arrow(descending, t.colors.accent_text));
            cell(t, &head, head.name.to_owned())
                .id(("map-head", at))
                .role(Role::Button)
                .aria_label(head.name)
                .test_support()
                .gap(t.spacing.xs)
                .cursor_pointer()
                .when(sorted.is_some(), |cell| cell.text_color(t.colors.text))
                .children(arrow)
                .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                    window.focus(&view.focus, cx);
                    view.sort_by(column, cx);
                }))
        });
        div()
            .flex()
            .flex_none()
            .h(t.spacing.row_height)
            .text_size(t.typography.tiny)
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(t.colors.muted)
            .bg(t.colors.rail)
            .border_b_1()
            .border_color(t.colors.line)
            .children(cells)
    }

    fn progress(&self) -> Option<SharedString> {
        match &self.index {
            Index::Running => {
                let done = u128::from(self.scanned) * 100 / u128::from(self.len.max(1));
                Some(format!("Reading map… {}%", done.min(100)).into())
            }
            _ => self.note.clone(),
        }
    }

    fn header_parts(&self) -> HeaderParts {
        let detail = self.loaded().zip(self.selected()).and_then(|(loaded, at)| {
            let text = summary(loaded, self.table(), at)?;
            Some(("map-cursor", text.into()))
        });
        let mode = self.loaded().map_or_else(
            || "Linker map".into(),
            |loaded| format!("Linker map · {}", flavor(loaded.model.flavor)).into(),
        );
        HeaderParts {
            title: self.title.clone(),
            mode,
            len: self.len,
            detail,
            status: self.progress().map(|text| ("map-progress", text)),
        }
    }

    fn header_view(&self, t: &Theme) -> Div {
        let bar = self.go_to.as_ref().map(|go_to| {
            let look = BarLook {
                id: "map-go-to",
                input_id: "map-go-to-input",
                error_id: "map-go-to-error",
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

    fn checks(&self) -> Vec<Check> {
        match &self.index {
            Index::Ready(loaded) => status(loaded),
            Index::Failed(error) => vec![Check {
                text: error.to_string(),
                is_bad: true,
            }],
            Index::Running => Vec::new(),
        }
    }

    fn status_line(&self, t: &Theme) -> impl IntoElement {
        let checks = self.checks();
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
                    .id(("map-status-part", at))
                    .test_support()
                    .flex()
                    .gap(t.spacing.sm)
                    .when(is_bad, |part| part.flex_none())
                    .when(!is_bad, |part| part.min_w_0().overflow_hidden())
                    .when(at > 0, |part| {
                        part.child(div().flex_none().text_color(t.colors.faint).child("·"))
                    })
                    .child(div().min_w_0().truncate().text_color(color).child(text))
            });
        div()
            .id("map-status")
            .role(Role::Status)
            .aria_label(label)
            .test_support()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.sm)
            .h(t.spacing.status_height)
            .px(t.spacing.xl)
            .min_w_0()
            .overflow_hidden()
            .border_t_1()
            .border_color(t.colors.line)
            .bg(t.colors.rail)
            .text_size(t.typography.small)
            .children(parts)
    }

    fn scope_bar(&self, t: &Theme) -> Option<impl IntoElement> {
        let label = self.query.scope.label()?;
        let count = format::count(self.rows.len() as u64, "item", "items");
        Some(
            div()
                .id("map-scope")
                .role(Role::Status)
                .aria_label(format!("Showing {label}, {count}"))
                .test_support()
                .flex()
                .flex_none()
                .items_center()
                .gap(t.spacing.sm)
                .h(t.spacing.row_height)
                .px(t.spacing.xl)
                .bg(t.colors.accent.opacity(0.12))
                .border_t_1()
                .border_color(t.colors.accent)
                .text_size(t.typography.small)
                .child(icon(IconName::Search, px(12.), t.colors.accent_text))
                .child(div().text_color(t.colors.muted).child("Only"))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(t.colors.text)
                        .child(label),
                )
                .child(div().flex_none().text_color(t.colors.muted).child(count))
                .child(div().flex_1())
                .child(div().text_color(t.colors.faint).child("Esc shows all")),
        )
    }

    fn measure(&mut self, window: &Window, t: &Theme) {
        let rows = (window.viewport_size().height / t.spacing.row_height).floor();
        self.page = if rows.is_finite() && rows > 6. {
            rows as usize - 6
        } else {
            1
        };
    }
}

impl Render for MapView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        self.measure(window, &t);
        let table = div()
            .id("map-table")
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(self.table_bar(&t, cx))
            .child(self.column_head(&t, cx))
            .child(self.list(cx))
            .child(self.drawers(&t, window, cx));
        let text = &self.query.text;
        let filter = (!text.is_empty())
            .then(|| filter_bar(&t, self.query.text.clone().into(), self.rows.len()));
        let body = div()
            .id("map-body")
            .key_context(TABLE_CONTEXT)
            .track_focus(&self.focus)
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(table)
            .children(self.scope_bar(&t))
            .children(filter);
        let root = div().id("map-view").key_context(CONTEXT);
        Self::keys(root, cx)
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .size_full()
            .bg(t.colors.surface)
            .text_size(t.typography.body)
            .child(self.header_view(&t))
            .child(Self::typing(body, cx))
            .child(self.status_line(&t))
    }
}
