//! Virtualised 8-column icon grid, set group headers, and search row builder.

use std::path::Path;
use std::rc::Rc;

use gpui_kit::base::{Scrollbar, TestSupportExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement, Role,
    SharedString, StatefulInteractiveElement, Styled, div, px,
};
use navo_icons::pack::SetInfo;
use navo_icons::registry::registry;
use navo_icons::search::{Hit, SetFilter, TagIndex, search};
use navo_icons::spec::{BrandColour, IconSpec};
use navo_l10n::{t as tr_args, tr};

use super::picker::PickerFilter;
use crate::primitives::{ListScroll, RowSemantics, Tooltip, VirtualList};
use crate::tokens::{self, Theme};
use crate::window_view::NavoWindow;
use crate::window_view::button_bar::icon::picker_icon_element;

pub(super) const ICONS_PER_ROW: usize = 8;
pub(super) const CAP_PER_SET: usize = 48;
pub(super) const GRID_ROW_HEIGHT: f32 = 40.;
const ICON_PREVIEW_PX: f32 = 20.;
const CELL_HEIGHT_PX: f32 = 34.;

/// One uniform-height row inside the virtualised icon picker list.
#[derive(Clone, Debug)]
pub(super) enum PickerRow {
    Header {
        set_id: String,
        title: String,
        matched: usize,
        total: u32,
        licence: String,
    },
    Icons {
        set_id: String,
        names: Vec<String>,
        start_index: usize,
    },
    MoreNote,
}

/// Searches `index` under `filter` and builds fixed-height rows of headers, 8-icon slices, and cap notes.
pub(super) fn build_rows(index: &TagIndex, query: &str, filter: PickerFilter) -> Rc<[PickerRow]> {
    let sets = registry().sets();
    let (hits, capped_filter) = match filter {
        PickerFilter::All => (search(index, query, &SetFilter::All, CAP_PER_SET + 1), true),
        PickerFilter::Downloaded => (
            search(index, query, &SetFilter::Downloaded, CAP_PER_SET + 1),
            true,
        ),
        PickerFilter::Lucide => (
            search(
                index,
                query,
                &SetFilter::Set("lucide".to_owned()),
                usize::MAX,
            ),
            false,
        ),
        PickerFilter::Brands => (
            search(
                index,
                query,
                &SetFilter::Set("brands".to_owned()),
                usize::MAX,
            ),
            false,
        ),
        PickerFilter::Tabler => (
            search_family(index, query, &sets, &["tabler", "tabler-filled"]),
            false,
        ),
        PickerFilter::Phosphor => (
            search_family(index, query, &sets, &["phosphor", "phosphor-fill"]),
            false,
        ),
    };

    let mut rows = Vec::new();
    let mut icon_index = 0usize;
    let mut start = 0usize;
    while start < hits.len() {
        let set_id = &hits[start].set;
        let mut end = start + 1;
        while end < hits.len() && hits[end].set == *set_id {
            end += 1;
        }
        let run = &hits[start..end];
        let capped = capped_filter && run.len() > CAP_PER_SET;
        let shown = if capped { &run[..CAP_PER_SET] } else { run };
        push_group(&mut rows, set_id, shown, capped, &sets, &mut icon_index);
        start = end;
    }
    Rc::from(rows)
}

fn search_family(index: &TagIndex, query: &str, sets: &[SetInfo], family_ids: &[&str]) -> Vec<Hit> {
    let mut out = Vec::new();
    for info in sets {
        if family_ids.contains(&info.id.as_str()) {
            out.extend(search(
                index,
                query,
                &SetFilter::Set(info.id.clone()),
                usize::MAX,
            ));
        }
    }
    out
}

fn push_group(
    rows: &mut Vec<PickerRow>,
    set_id: &str,
    shown: &[Hit],
    capped: bool,
    sets: &[SetInfo],
    icon_index: &mut usize,
) {
    let info = sets.iter().find(|s| s.id == set_id);
    rows.push(PickerRow::Header {
        set_id: set_id.to_owned(),
        title: set_display_name(set_id),
        matched: shown.len(),
        total: info.map_or(0, |s| s.count),
        licence: info.map_or_else(String::new, |s| s.licence.clone()),
    });
    for chunk in shown.chunks(ICONS_PER_ROW) {
        let names: Vec<String> = chunk.iter().map(|hit| hit.name.clone()).collect();
        let count = names.len();
        rows.push(PickerRow::Icons {
            set_id: set_id.to_owned(),
            names,
            start_index: *icon_index,
        });
        *icon_index += count;
    }
    if capped {
        rows.push(PickerRow::MoreNote);
    }
}

struct GridContext<'a> {
    brand_colour: bool,
    view: &'a Entity<NavoWindow>,
    theme: &'a Theme,
    cx: &'a App,
}

/// Renders the virtualised uniform-height icon grid inside a bordered field container.
pub(super) fn picker_virtual_list(
    rows: Rc<[PickerRow]>,
    scroll: &ListScroll,
    brand_colour: bool,
    view: Entity<NavoWindow>,
    t: &Theme,
) -> AnyElement {
    let rows_render = Rc::clone(&rows);
    let rows_meta = Rc::clone(&rows);
    let list = VirtualList::new(
        "button-bar-picker-list",
        rows.len(),
        scroll,
        move |range, _, cx| {
            let theme = tokens::current(cx);
            let ctx = GridContext {
                brand_colour,
                view: &view,
                theme,
                cx,
            };
            range
                .map(|idx| render_row(idx, &rows_render[idx], &ctx))
                .collect()
        },
    )
    .row_height(px(GRID_ROW_HEIGHT))
    .label(tr("button-bar-picker-list-label"))
    .describe_rows(move |idx| RowSemantics {
        label: row_semantics_label(&rows_meta[idx]),
        is_selected: None,
        is_active_descendant: false,
    });

    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .rounded(t.radii.md)
        .border_1()
        .border_color(t.surfaces.card_edge)
        .bg(t.surfaces.field)
        .p(t.spacing.xs)
        .child(list)
        .into_any_element()
}

fn row_semantics_label(row: &PickerRow) -> Option<SharedString> {
    match row {
        PickerRow::Header {
            title,
            matched,
            total,
            licence,
            ..
        } => Some(header_aria_label(title, *matched, *total, licence).into()),
        PickerRow::Icons { .. } => None,
        PickerRow::MoreNote => Some(tr("button-bar-picker-keep-typing").into()),
    }
}

fn render_row(row_index: usize, row: &PickerRow, ctx: &GridContext<'_>) -> AnyElement {
    match row {
        PickerRow::Header {
            title,
            matched,
            total,
            licence,
            ..
        } => render_header_row(row_index, title, *matched, *total, licence, ctx.theme),
        PickerRow::Icons {
            set_id,
            names,
            start_index,
        } => render_icons_row(row_index, set_id, names, *start_index, ctx),
        PickerRow::MoreNote => render_more_row(row_index, ctx.theme),
    }
}

fn render_header_row(
    row_index: usize,
    title: &str,
    matched: usize,
    total: u32,
    licence: &str,
    t: &Theme,
) -> AnyElement {
    let count_text = count_label(matched, total);
    let aria = header_aria_label(title, matched, total, licence);
    div()
        .id(("button-bar-picker-group", row_index))
        .role(Role::Heading)
        .aria_label(aria)
        .test_support()
        .w_full()
        .h(px(GRID_ROW_HEIGHT))
        .pl(t.spacing.sm)
        .pr(t.spacing.sm + Scrollbar::width())
        .flex()
        .items_center()
        .justify_between()
        .gap(t.spacing.sm)
        .child(
            div()
                .flex()
                .items_center()
                .gap(t.spacing.sm)
                .min_w_0()
                .child(
                    div()
                        .truncate()
                        .text_size(t.typography.small)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(t.colors.text)
                        .child(title.to_owned()),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(t.typography.small)
                        .text_color(t.colors.muted)
                        .child(count_text),
                ),
        )
        .when(!licence.is_empty(), |el| {
            el.child(
                div()
                    .id(("button-bar-picker-licence", row_index))
                    .test_support()
                    .flex_none()
                    .px(t.spacing.xs)
                    .py(t.spacing.xxs)
                    .rounded(t.radii.sm)
                    .border_1()
                    .border_color(t.surfaces.card_edge)
                    .bg(t.surfaces.card)
                    .text_size(t.typography.small)
                    .text_color(t.colors.muted)
                    .child(licence.to_owned()),
            )
        })
        .into_any_element()
}

fn render_icons_row(
    row_index: usize,
    set_id: &str,
    names: &[String],
    start_index: usize,
    ctx: &GridContext<'_>,
) -> AnyElement {
    let t = ctx.theme;
    let empty_slots = ICONS_PER_ROW.saturating_sub(names.len());
    div()
        .id(("button-bar-picker-row", row_index))
        .w_full()
        .h(px(GRID_ROW_HEIGHT))
        .pl(t.spacing.xs)
        .pr(t.spacing.xs + Scrollbar::width())
        .flex()
        .items_center()
        .gap(t.spacing.xs)
        .children(
            names
                .iter()
                .enumerate()
                .map(|(col, name)| icon_cell(start_index + col, set_id, name, ctx)),
        )
        .children((0..empty_slots).map(|_| div().flex_1().min_w_0()))
        .into_any_element()
}

fn icon_cell(global_index: usize, set_id: &str, name: &str, ctx: &GridContext<'_>) -> AnyElement {
    let t = ctx.theme;
    let spec = make_icon_spec(set_id, name, ctx.brand_colour);
    let (resolved, _) = registry().resolve(&spec, Path::new(""));
    let hover = t.colors.hover;
    let tip: SharedString = name.to_owned().into();
    let aria = format!("{set_id}:{name}");
    let view = ctx.view.clone();
    div()
        .id(("button-bar-picker-icon", global_index))
        .role(Role::Button)
        .aria_label(aria)
        .test_support()
        .flex_1()
        .min_w_0()
        .h(px(CELL_HEIGHT_PX))
        .rounded(t.radii.sm)
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        .on_click(move |_, window, cx| {
            let chosen = spec.clone();
            view.update(cx, |view, cx| view.pick_editor_icon(chosen, window, cx));
        })
        .child(picker_icon_element(&resolved, px(ICON_PREVIEW_PX), ctx.cx))
        .into_any_element()
}

fn render_more_row(row_index: usize, t: &Theme) -> AnyElement {
    let note = tr("button-bar-picker-keep-typing");
    div()
        .id(("button-bar-picker-more", row_index))
        .role(Role::Note)
        .aria_label(note.clone())
        .test_support()
        .w_full()
        .h(px(GRID_ROW_HEIGHT))
        .pl(t.spacing.sm)
        .pr(t.spacing.sm + Scrollbar::width())
        .flex()
        .items_center()
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(note)
        .into_any_element()
}

fn make_icon_spec(set_id: &str, name: &str, brand_colour: bool) -> IconSpec {
    if set_id == "brands" {
        IconSpec::Brand {
            name: name.to_owned(),
            colour: if brand_colour {
                BrandColour::Brand
            } else {
                BrandColour::Theme
            },
        }
    } else {
        IconSpec::Set {
            set: set_id.to_owned(),
            name: name.to_owned(),
            tone: None,
        }
    }
}

fn count_label(matched: usize, total: u32) -> String {
    tr_args!(
        "button-bar-picker-group-count",
        shown = matched,
        total = total
    )
}

fn header_aria_label(title: &str, matched: usize, total: u32, licence: &str) -> String {
    let count = count_label(matched, total);
    if licence.is_empty() {
        format!("{title} · {count}")
    } else {
        format!("{title} · {count} · {licence}")
    }
}

fn set_display_name(id: &str) -> String {
    match id {
        "lucide" => tr("button-bar-set-lucide"),
        "tabler" => tr("button-bar-set-tabler"),
        "tabler-filled" => tr("button-bar-set-tabler-filled"),
        "phosphor" => tr("button-bar-set-phosphor"),
        "phosphor-fill" => tr("button-bar-set-phosphor-fill"),
        "brands" => tr("button-bar-set-brands"),
        "fluent" => tr("button-bar-set-fluent"),
        "fluent-filled" => tr("button-bar-set-fluent-filled"),
        "material" => tr("button-bar-set-material"),
        "carbon" => tr("button-bar-set-carbon"),
        other => other.to_owned(),
    }
}
