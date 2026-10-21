use gpui_kit::base::TestSupportExt;
use gpui_kit::{ClickEvent, Role, Stateful, WeakEntity};

use super::keys::{CONTEXT, TABLE_CONTEXT};
use super::text::{Check, flavor, row_label, status, summary};
use super::{Focus, Index, MapView};
use crate::file_list::filter_bar;
use crate::format;
use crate::prelude::*;
use crate::primitives::{IconName, VirtualList, icon};
use crate::tokens::{self, Theme};
use crate::window_view::viewer::rich_header::{HeaderParts, header};
use crate::window_view::viewer::search::bar::{BarLook, bar_row};
use crate::window_view::viewer::space::{Part, RichLook, host_context, rich_layout, space_keys};

fn select_row(
    view: WeakEntity<MapView>,
    row: usize,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) {
    move |_, window, cx| {
        let _ = view.update(cx, |view, cx| {
            window.focus(&view.focus, cx);
            view.focus_in = Focus::Content;
            view.go_address = None;
            view.clear_pick(cx);
            view.rich.part = Part::Table;
            view.move_cursor(row, cx);
            view.open_inspector_if_asked(cx);
        });
    }
}

impl MapView {
    fn row_view(&self, row: usize, view: &WeakEntity<Self>, cx: &App) -> AnyElement {
        let t = tokens::current(cx);
        let (Some(loaded), Some(at)) = (self.loaded(), self.rows.get(row)) else {
            return div().into_any_element();
        };
        let at = *at as usize;
        let table = self.table();
        let cells = super::text::cells(loaded, table, at);
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
            .children(self.row_cells(row, &cells, cx))
            .children(self.share_cell(t, at))
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
                let this = entity.read(cx);
                range.map(|row| this.row_view(row, &view, cx)).collect()
            },
        )
        .label(self.table().name())
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
        let mut checks = match &self.index {
            Index::Ready(loaded) => status(loaded),
            Index::Failed(error) => vec![Check {
                text: error.to_string(),
                is_bad: true,
            }],
            Index::Running => Vec::new(),
        };
        let (picked, outside) = self.rich_checks();
        checks.splice(0..0, picked);
        checks.extend(outside);
        checks
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

impl MapView {
    fn table_view(&self, t: &Theme, window: &mut Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let is_rich = self.is_rich();
        let head = div().flex_1().min_w_0().children(self.column_head());
        let head = match self.share_head(t) {
            Some(share) => div().flex().flex_none().child(head).child(share),
            None => div().child(head),
        };
        div()
            .id("map-table")
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(self.table_bar(t, cx))
            .child(head)
            .child(self.list(cx))
            .when(!is_rich, |table| table.child(self.drawers(t, window, cx)))
    }

    fn rich_body(&self, table: AnyElement, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let t = &tokens::current(cx).clone();
        let look = RichLook {
            parts: self.layout,
            width: window.viewport_size().width,
            file_bytes: None,
            table,
        };
        let drawers = self.drawers(t, window, cx);
        div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(rich_layout(t, &self.rich, look, cx))
            .child(drawers)
    }
}

impl Render for MapView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        self.measure(window, &t);
        self.ensure_columns(window, cx);
        let table = self.table_view(&t, window, cx);
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
            .min_h_0();
        let body = if self.is_rich() {
            let table = div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .child(table)
                .children(self.scope_bar(&t))
                .children(filter)
                .into_any_element();
            body.child(self.rich_body(table, window, cx))
        } else {
            body.child(table)
                .children(self.scope_bar(&t))
                .children(filter)
        };
        let root = div().id("map-view").key_context(host_context(CONTEXT));
        space_keys(Self::keys(root, cx), cx)
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
