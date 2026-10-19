use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::{ClickEvent, Role, Stateful, WeakEntity};

use super::inspect::inspector_body;
use super::text::{span, stat_name};
use super::{Focus, TraceView};
use crate::format;
use crate::prelude::*;
use crate::primitives::VirtualList;
use crate::side_sheet::{SheetFrame, Side, SideSheet};
use crate::tokens::{self, Theme};

const NAVIGATOR_WIDTH: f32 = 360.;
const INSPECTOR_WIDTH: f32 = 380.;
const ITEM_HEIGHT: f32 = 40.;

fn frame(t: &Theme, window: &Window, (width, side): (f32, Side)) -> SheetFrame {
    let body = window.viewport_size().width - t.spacing.md * 2.;
    SheetFrame::new(body, px(width), side.other()).covering_passive_card()
}

fn pick(view: WeakEntity<TraceView>, at: usize) -> impl Fn(&ClickEvent, &mut Window, &mut App) {
    move |_, window, cx| {
        let _ = view.update(cx, |view, cx| {
            window.focus(&view.focus, cx);
            view.focus_in = Focus::Navigator;
            view.pick_nav(at, cx);
        });
    }
}

fn small(t: &Theme) -> Div {
    div()
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
}

impl TraceView {
    fn item_texts(&self, key: Option<u32>) -> (String, String, String) {
        let Some(stat) = key.and_then(|key| self.trace.stats.get(key as usize)) else {
            let frames = format::bytes(self.trace.rows());
            return (
                "All rows".to_owned(),
                "clears the ID filter".to_owned(),
                frames,
            );
        };
        let detail = stat
            .mean_period_ns()
            .map_or_else(|| "once".to_owned(), |ns| format!("every {}", span(ns)));
        (
            stat_name(stat, self.trace.notation().base),
            detail,
            format::bytes(stat.count),
        )
    }

    fn item_row(&self, t: &Theme, at: usize) -> ObservedElement<Stateful<Div>> {
        let key = self.nav.items.get(at).copied().flatten();
        let (name, detail, count) = self.item_texts(key);
        let is_selected = at == self.nav.selected;
        let is_active = self.filter.as_ref().map(|filter| filter.key) == key;
        div()
            .id(("trace-id", at))
            .role(Role::ListItem)
            .aria_label(format!("{name}, {count}"))
            .aria_selected(is_selected)
            .test_support()
            .flex()
            .items_center()
            .gap(t.spacing.md)
            .w_full()
            .h(px(ITEM_HEIGHT))
            .px(t.spacing.xl)
            .cursor_pointer()
            .when(is_selected, |row| row.bg(t.colors.selection))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .font_family(t.typography.mono_font.clone())
                            .when(is_active, |name| name.text_color(t.colors.accent_text))
                            .child(name),
                    )
                    .child(small(t).truncate().child(detail)),
            )
            .child(
                div()
                    .flex_none()
                    .font_family(t.typography.numbers_font.clone())
                    .text_color(t.colors.muted)
                    .child(count),
            )
    }

    fn navigator_list(&self, cx: &mut Context<Self>) -> VirtualList {
        let view = cx.weak_entity();
        VirtualList::new(
            "trace-ids",
            self.nav.items.len(),
            &self.nav_scroll,
            move |range, _, cx| {
                let Some(entity) = view.upgrade() else {
                    return Vec::new();
                };
                let t = tokens::current(cx).clone();
                let this = entity.read(cx);
                range
                    .map(|at| {
                        this.item_row(&t, at)
                            .on_click(pick(view.clone(), at))
                            .into_any_element()
                    })
                    .collect()
            },
        )
        .row_height(px(ITEM_HEIGHT))
        .label("IDs")
    }

    fn navigator_title(&self, t: &Theme) -> Div {
        let ids = format::count(self.trace.stats.len() as u64, "ID", "IDs");
        let sort = if self.nav.by_id { "by ID" } else { "by count" };
        let typed = if self.nav.typed.is_empty() {
            "type to filter".to_owned()
        } else {
            format!("filter: {}", self.nav.typed)
        };
        div()
            .flex()
            .flex_col()
            .min_w_0()
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(t.spacing.md)
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("IDs"))
                    .child(small(t).child(format!("{ids} · {sort}"))),
            )
            .child(
                small(t)
                    .id("trace-id-filter")
                    .aria_label(typed.clone())
                    .test_support()
                    .font_family(t.typography.mono_font.clone())
                    .child(typed),
            )
    }

    fn navigator(&self, t: &Theme, window: &Window, cx: &mut Context<Self>) -> SideSheet {
        let this = cx.weak_entity();
        SideSheet::new(
            "trace-navigator",
            &self.navigator,
            frame(t, window, (NAVIGATOR_WIDTH, Side::Left)),
        )
        .label("IDs")
        .header(self.navigator_title(t))
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
        .hint(&["enter"], "Filter")
        .hint(&["alt", "s"], "Sort")
        .hint(&["escape"], "Close")
    }

    fn inspector(&self, t: &Theme, window: &Window, cx: &mut Context<Self>) -> SideSheet {
        let this = cx.weak_entity();
        let title = match self.actual(self.cursor) {
            Some(row) => format!("Row {}", format::bytes(row + 1)),
            None => "No row".to_owned(),
        };
        SideSheet::new(
            "trace-inspector",
            &self.inspector,
            frame(t, window, (INSPECTOR_WIDTH, Side::Right)),
        )
        .label("Inspector")
        .header(div().font_weight(FontWeight::SEMIBOLD).child(title))
        .close("ctrl-i", move |_, cx| {
            let _ = this.update(cx, |view, cx| view.toggle_inspector(cx));
        })
        .body(inspector_body(self, t))
    }

    pub(super) fn drawers(&self, t: &Theme, window: &Window, cx: &mut Context<Self>) -> Div {
        let navigator = self
            .navigator
            .read(cx)
            .is_shown(cx)
            .then(|| self.navigator(t, window, cx));
        let inspector = self
            .inspector
            .read(cx)
            .is_shown(cx)
            .then(|| self.inspector(t, window, cx));
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
