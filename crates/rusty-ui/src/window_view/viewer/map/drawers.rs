use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::{ClickEvent, Role, Stateful, WeakEntity};

use super::blocks::{Block, Row, blocks, file_name, spoken};
use super::facts::Inspection;
use super::loaded::Loaded;
use super::nav::NavItem;
use super::text::percent;
use super::{Focus, MapView};
use crate::format;
use crate::prelude::*;
use crate::primitives::VirtualList;
use crate::side_sheet::{SheetFrame, Side, SideSheet};
use crate::tokens::{self, Theme};

const NAVIGATOR_WIDTH: f32 = 360.;
const INSPECTOR_WIDTH: f32 = 380.;
const ITEM_HEIGHT: f32 = 44.;

fn frame(t: &Theme, window: &Window, (width, side): (f32, Side)) -> SheetFrame {
    let body = window.viewport_size().width - t.spacing.md * 2.;
    SheetFrame::new(body, px(width), side.other()).covering_passive_card()
}

fn caption(t: &Theme, text: impl Into<SharedString>) -> Div {
    div()
        .text_size(t.typography.tiny)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(t.colors.muted)
        .child(text.into())
}

fn pick(view: WeakEntity<MapView>, at: usize) -> impl Fn(&ClickEvent, &mut Window, &mut App) {
    move |_, window, cx| {
        let _ = view.update(cx, |view, cx| {
            window.focus(&view.focus, cx);
            view.focus_in = Focus::Navigator;
            view.pick_item(at, cx);
        });
    }
}

fn item_text(loaded: &Loaded, item: NavItem) -> (String, String, String) {
    match item {
        NavItem::All => (
            "Whole map".to_owned(),
            format::size(loaded.used()),
            format!(
                "{} · {}",
                format::count(loaded.regions.len() as u64, "region", "regions"),
                format::count(loaded.objects.len() as u64, "object", "objects")
            ),
        ),
        NavItem::Region(at) => loaded
            .regions
            .get(at)
            .map(|region| {
                (
                    region.name.clone(),
                    format!(
                        "{} / {}",
                        format::size(region.used),
                        format::size(region.length())
                    ),
                    format!(
                        "{} used · {} free",
                        percent(region.used, region.length()),
                        format::size(region.free)
                    ),
                )
            })
            .unwrap_or_default(),
        NavItem::Object(at) => loaded
            .objects
            .get(at)
            .map(|object| {
                (
                    file_name(&object.name).to_owned(),
                    format::size(object.bytes),
                    if object.regions.is_empty() {
                        object.name.clone()
                    } else {
                        object.regions.join(", ")
                    },
                )
            })
            .unwrap_or_default(),
    }
}

struct ItemLook {
    at: usize,
    is_selected: bool,
    starts_objects: bool,
}

fn item_row(
    t: &Theme,
    loaded: &Loaded,
    (item, look): (NavItem, ItemLook),
) -> ObservedElement<Stateful<Div>> {
    let (name, size, detail) = item_text(loaded, item);
    let kind = match item {
        NavItem::All => "All",
        NavItem::Region(_) => "Region",
        NavItem::Object(_) => "Object",
    };
    div()
        .id(("map-item", look.at))
        .role(Role::ListItem)
        .aria_label(format!("{kind} {name}, {size}, {detail}"))
        .aria_selected(look.is_selected)
        .test_support()
        .flex()
        .flex_col()
        .justify_center()
        .w_full()
        .h(px(ITEM_HEIGHT))
        .pl(t.spacing.xl)
        .pr(t.spacing.xl * 2.)
        .cursor_pointer()
        .when(look.starts_objects, |row| {
            row.border_t_1().border_color(t.colors.line)
        })
        .when(look.is_selected, |row| row.bg(t.colors.selection))
        .child(
            div()
                .flex()
                .gap(t.spacing.md)
                .child(div().flex_1().min_w_0().truncate().child(name))
                .child(
                    div()
                        .flex_none()
                        .font_family(t.typography.numbers_font.clone())
                        .text_color(t.colors.muted)
                        .child(size),
                ),
        )
        .child(
            div()
                .truncate()
                .font_family(t.typography.mono_font.clone())
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(detail),
        )
}

fn pair(t: &Theme, (label, value): (&'static str, String)) -> Div {
    div()
        .flex()
        .justify_between()
        .gap(t.spacing.md)
        .child(div().flex_none().text_color(t.colors.muted).child(label))
        .child(div().min_w_0().truncate().child(value))
}

fn size_row(t: &Theme, (name, detail, bytes): (&str, &str, u64)) -> Div {
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .gap(t.spacing.md)
                .child(div().flex_1().min_w_0().truncate().child(name.to_owned()))
                .child(
                    div()
                        .flex_none()
                        .font_family(t.typography.numbers_font.clone())
                        .child(format::bytes(bytes)),
                ),
        )
        .when(!detail.is_empty(), |row| {
            row.child(
                div()
                    .truncate()
                    .font_family(t.typography.mono_font.clone())
                    .text_size(t.typography.small)
                    .text_color(t.colors.muted)
                    .child(detail.to_owned()),
            )
        })
}

fn row_view(t: &Theme, row: &Row) -> Div {
    match row {
        Row::Pair(label, value) => pair(t, (label, value.clone())),
        Row::Item {
            name,
            detail,
            bytes,
        } => size_row(t, (name, detail, *bytes)),
        Row::Note(note) => div().text_color(t.colors.muted).child(note.clone()),
    }
}

fn block_view(t: &Theme, block: &Block) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.sm)
        .child(caption(t, block.title))
        .children(block.rows.iter().map(|row| row_view(t, row)))
}

impl MapView {
    fn navigator_list(&self, cx: &mut Context<Self>) -> VirtualList {
        let view = cx.weak_entity();
        VirtualList::new(
            "map-items",
            self.nav_items.len(),
            &self.nav_scroll,
            move |range, _, cx| {
                let Some(entity) = view.upgrade() else {
                    return Vec::new();
                };
                let t = tokens::current(cx).clone();
                let this = entity.read(cx);
                let Some(loaded) = this.loaded() else {
                    return Vec::new();
                };
                range
                    .filter_map(|at| {
                        let item = *this.nav_items.get(at)?;
                        let look = ItemLook {
                            at,
                            is_selected: at == this.nav_cursor,
                            starts_objects: at > 0
                                && matches!(item, NavItem::Object(_))
                                && matches!(
                                    this.nav_items.get(at - 1),
                                    Some(NavItem::Region(_) | NavItem::All)
                                ),
                        };
                        Some(
                            item_row(&t, loaded, (item, look))
                                .on_click(pick(view.clone(), at))
                                .into_any_element(),
                        )
                    })
                    .collect()
            },
        )
        .row_height(px(ITEM_HEIGHT))
        .label("Regions and objects")
    }

    fn navigator(&self, t: &Theme, window: &Window, cx: &mut Context<Self>) -> Option<SideSheet> {
        self.loaded()?;
        let this = cx.weak_entity();
        let title = div()
            .font_weight(FontWeight::SEMIBOLD)
            .child("Regions and objects");
        Some(
            SideSheet::new(
                "map-navigator",
                &self.navigator,
                frame(t, window, (NAVIGATOR_WIDTH, Side::Left)),
            )
            .label("Regions and objects")
            .header(title)
            .close("ctrl-b", move |_, cx| {
                let _ = this.update(cx, |view, cx| view.close_navigator(cx));
            })
            .body(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .py(t.spacing.sm)
                    .child(self.navigator_list(cx)),
            )
            .hint(&["↑", "↓"], "Move")
            .hint(&["enter"], "Show only this")
            .hint(&["escape"], "Close"),
        )
    }

    fn inspector_body(&self, t: &Theme) -> AnyElement {
        let body = div()
            .id("map-inspector-text")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .gap(t.spacing.xl)
            .p(t.spacing.xl);
        let Some(inspection) = &self.inspection else {
            let none = div().text_color(t.colors.muted).child("Nothing selected");
            return body.child(none).into_any_element();
        };
        let blocks = blocks(inspection, self.go_address);
        body.aria_label(spoken(&blocks))
            .test_support()
            .children(blocks.iter().map(|block| block_view(t, block)))
            .into_any_element()
    }

    fn inspector(&self, t: &Theme, window: &Window, cx: &mut Context<Self>) -> Option<SideSheet> {
        self.loaded()?;
        let this = cx.weak_entity();
        let name = match &self.inspection {
            Some(Inspection::Region { region, .. }) => region.name.clone(),
            Some(Inspection::Section { section, .. }) => section.name.clone(),
            Some(Inspection::Symbol { symbol, .. }) => symbol.name.clone(),
            None => String::new(),
        };
        let title = div()
            .min_w_0()
            .truncate()
            .font_weight(FontWeight::SEMIBOLD)
            .child(name);
        Some(
            SideSheet::new(
                "map-inspector",
                &self.inspector,
                frame(t, window, (INSPECTOR_WIDTH, Side::Right)),
            )
            .label("Inspector")
            .header(title)
            .close("ctrl-i", move |_, cx| {
                let _ = this.update(cx, |view, cx| view.toggle_inspector(cx));
            })
            .body(self.inspector_body(t)),
        )
    }

    pub(super) fn drawers(&self, t: &Theme, window: &Window, cx: &mut Context<Self>) -> Div {
        let navigator = if self.navigator.read(cx).is_shown(cx) {
            self.navigator(t, window, cx)
        } else {
            None
        };
        let inspector = if self.inspector.read(cx).is_shown(cx) {
            self.inspector(t, window, cx)
        } else {
            None
        };
        div()
            .absolute()
            .top(t.spacing.xs)
            .bottom(t.spacing.xs)
            .left(t.spacing.md)
            .right(t.spacing.md)
            .children(navigator)
            .children(inspector)
    }
}
