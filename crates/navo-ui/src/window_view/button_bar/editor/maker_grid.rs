//! Virtualised 8-column Lucide glyph grid for the button bar icon maker.

use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, Context, Entity, InteractiveElement, IntoElement, ParentElement, Role,
    SharedString, StatefulInteractiveElement, Styled, div, px,
};
use navo_icons::registry::registry;
use navo_l10n::tr;

use super::maker_state::GLYPHS_PER_ROW;
use crate::primitives::{ListScroll, RowSemantics, Tooltip, VirtualList};
use crate::tokens::{self, Theme};
use crate::window_view::NavoWindow;
use crate::window_view::button_bar::BarIcon;
use crate::window_view::button_bar::icon::icon_element;

const GLYPH_ROW_HEIGHT: f32 = 36.;
const GLYPH_CELL_HEIGHT: f32 = 30.;
const GLYPH_CELL_ICON_PX: f32 = 18.;
const GLYPH_LIST_MIN_HEIGHT: f32 = 120.;

/// Renders the virtualised 8-column Lucide glyph grid (or loading/empty hint).
pub(super) fn glyph_grid_area(
    rows: Rc<[Vec<String>]>,
    scroll: &ListScroll,
    selected_glyph: &str,
    loading: bool,
    t: &Theme,
    cx: &Context<NavoWindow>,
) -> AnyElement {
    if loading {
        return hint_box(tr("button-bar-maker-glyph-loading"), t);
    }
    if rows.is_empty() {
        return hint_box(tr("button-bar-maker-glyph-none"), t);
    }
    let rows_render = Rc::clone(&rows);
    let rows_meta = Rc::clone(&rows);
    let selected = selected_glyph.to_owned();
    let view = cx.entity();
    let list = VirtualList::new(
        "button-bar-maker-glyph-list",
        rows.len(),
        scroll,
        move |range, _, cx| {
            let theme = tokens::current(cx);
            range
                .map(|row_idx| {
                    render_glyph_row(row_idx, &rows_render[row_idx], &selected, &view, theme, cx)
                })
                .collect()
        },
    )
    .row_height(px(GLYPH_ROW_HEIGHT))
    .label(tr("button-bar-maker-glyph-list-label"))
    .describe_rows(move |idx| RowSemantics {
        label: rows_meta[idx].first().cloned().map(Into::into),
        is_selected: None,
        is_active_descendant: false,
    });

    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(GLYPH_LIST_MIN_HEIGHT))
        .min_w_0()
        .rounded(t.radii.md)
        .border_1()
        .border_color(t.surfaces.card_edge)
        .bg(t.surfaces.field)
        .p(t.spacing.xs)
        .child(list)
        .into_any_element()
}

fn hint_box(message: String, t: &Theme) -> AnyElement {
    div()
        .flex_1()
        .min_h(px(GLYPH_LIST_MIN_HEIGHT))
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(message)
        .into_any_element()
}

fn render_glyph_row(
    row_idx: usize,
    names: &[String],
    selected: &str,
    view: &Entity<NavoWindow>,
    t: &Theme,
    cx: &App,
) -> AnyElement {
    let start_index = row_idx * GLYPHS_PER_ROW;
    let empty_slots = GLYPHS_PER_ROW.saturating_sub(names.len());
    div()
        .id(("button-bar-maker-glyph-row", row_idx))
        .w_full()
        .h(px(GLYPH_ROW_HEIGHT))
        .px(t.spacing.xs)
        .flex()
        .items_center()
        .gap(t.spacing.xs)
        .children(
            names.iter().enumerate().map(|(col, name)| {
                glyph_cell(start_index + col, name, name == selected, view, t, cx)
            }),
        )
        .children((0..empty_slots).map(|_| div().flex_1().min_w_0()))
        .into_any_element()
}

fn glyph_cell(
    global_index: usize,
    name: &str,
    selected: bool,
    view: &Entity<NavoWindow>,
    t: &Theme,
    cx: &App,
) -> AnyElement {
    let icon = match registry().body("lucide", name) {
        Some(svg) => BarIcon::Mono {
            svg,
            tone: None,
            brand: None,
        },
        None => BarIcon::Missing { dashed: false },
    };
    let hover = t.colors.hover;
    let tip: SharedString = name.to_owned().into();
    let chosen = name.to_owned();
    let view = view.clone();
    div()
        .id(("button-bar-maker-glyph", global_index))
        .role(Role::Button)
        .aria_label(name.to_owned())
        .aria_selected(selected)
        .test_support()
        .flex_1()
        .min_w_0()
        .h(px(GLYPH_CELL_HEIGHT))
        .rounded(t.radii.sm)
        .border_1()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .when(selected, |d| {
            d.bg(t.colors.selection).border_color(t.colors.accent)
        })
        .when(!selected, |d| {
            d.border_color(t.colors.line.opacity(0.))
                .hover(move |s| s.bg(hover))
        })
        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        .on_click(move |_, _, cx| {
            let next = chosen.clone();
            view.update(cx, |view, cx| view.set_maker_glyph(next, cx));
        })
        .child(icon_element(&icon, px(GLYPH_CELL_ICON_PX), cx))
        .into_any_element()
}
