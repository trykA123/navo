use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::{ClickEvent, Role, Stateful};

use super::MapView;
use super::select::Table;
use crate::format;
use crate::prelude::*;
use crate::tokens::Theme;

fn segment(
    t: &Theme,
    table: Table,
    is_shown: bool,
    cx: &mut Context<MapView>,
) -> ObservedElement<Stateful<Div>> {
    div()
        .id(("map-table", table.index()))
        .role(Role::Button)
        .aria_label(table.name())
        .test_support()
        .flex()
        .items_center()
        .h(t.spacing.tab_height - t.spacing.sm)
        .px(t.spacing.lg)
        .rounded(t.radii.md)
        .cursor_pointer()
        .text_size(t.typography.small)
        .when(is_shown, |segment| {
            segment
                .bg(t.colors.selection)
                .text_color(t.colors.text)
                .font_weight(FontWeight::SEMIBOLD)
        })
        .when(!is_shown, |segment| {
            segment
                .text_color(t.colors.muted)
                .hover(|style| style.bg(t.colors.hover))
        })
        .child(table.name())
        .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
            window.focus(&view.focus, cx);
            view.show_table(table, cx);
        }))
}

impl MapView {
    fn count_text(&self) -> String {
        let Some(loaded) = self.loaded() else {
            return String::new();
        };
        let total = self.table().len(loaded) as u64;
        let shown = self.rows.len() as u64;
        let noun = match self.table() {
            Table::Memory => ("region", "regions"),
            Table::Sections => ("section", "sections"),
            Table::Symbols => ("symbol", "symbols"),
        };
        if shown == total {
            format::count(total, noun.0, noun.1)
        } else {
            format!(
                "{} of {}",
                format::bytes(shown),
                format::count(total, noun.0, noun.1)
            )
        }
    }

    pub(super) fn table_bar(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let shown = self.table();
        let count = self.count_text();
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.md)
            .h(t.spacing.tab_height)
            .px(t.spacing.xl)
            .child(
                div()
                    .id("map-tables")
                    .role(Role::TabList)
                    .aria_label("Table")
                    .test_support()
                    .flex()
                    .gap(t.spacing.xs)
                    .children(Table::ALL.map(|table| segment(t, table, table == shown, cx))),
            )
            .child(div().flex_1())
            .child(
                div()
                    .id("map-count")
                    .aria_label(count.clone())
                    .test_support()
                    .text_size(t.typography.small)
                    .text_color(t.colors.muted)
                    .child(count),
            )
    }
}
