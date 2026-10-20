use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::{ClickEvent, Role, Stateful, WeakEntity};
use rusty_automotive::Image;

use super::entries;
use super::text::{padding_gaps, percent, problem, span, start, whole};
use super::{Focus, MemoryView, Ready};
use crate::format;
use crate::prelude::*;
use crate::primitives::{IconName, VirtualList, icon};
use crate::side_sheet::{SheetFrame, Side, SideSheet};
use crate::tokens::{self, Theme};

const NAVIGATOR_WIDTH: f32 = 340.;
const INSPECTOR_WIDTH: f32 = 380.;
const ITEM_HEIGHT: f32 = 44.;
const SHOWN_PROBLEMS: usize = 5;

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

fn pick(view: WeakEntity<MemoryView>, item: usize) -> impl Fn(&ClickEvent, &mut Window, &mut App) {
    move |_, window, cx| {
        let _ = view.update(cx, |view, cx| {
            window.focus(&view.focus, cx);
            view.focus_in = Focus::Navigator;
            view.pick_item(item, cx);
        });
    }
}

fn item_title(t: &Theme, (name, size): (String, String), is_gap: bool) -> Div {
    let color = if is_gap {
        t.colors.muted
    } else {
        t.colors.text
    };
    div()
        .flex()
        .gap(t.spacing.md)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(color)
                .child(name),
        )
        .child(
            div()
                .flex_none()
                .font_family(t.typography.numbers_font.clone())
                .text_color(t.colors.muted)
                .child(size),
        )
}

fn item_row(
    t: &Theme,
    ready: &Ready,
    at: usize,
    is_selected: bool,
) -> ObservedElement<Stateful<Div>> {
    let Some(item) = ready.items.get(at) else {
        return div().id(("memory-item", at)).test_support();
    };
    let span = &item.span;
    let name = entries::name(&ready.items, at);
    let size = format::size(span.end - span.start);
    let is_gap = item.is_gap;
    let place = super::text::span(span, ready.wide);
    let place = if item.folded > 0 {
        format!("{place} · {}", padding_gaps(item.folded))
    } else {
        place
    };
    let label = format!("{name}, {place}, {size}");
    div()
        .id(("memory-item", at))
        .role(Role::ListItem)
        .aria_label(label)
        .aria_selected(is_selected)
        .test_support()
        .flex()
        .flex_col()
        .justify_center()
        .w_full()
        .h(px(ITEM_HEIGHT))
        .px(t.spacing.xl)
        .cursor_pointer()
        .when(is_selected, |row| row.bg(t.colors.selection))
        .child(item_title(t, (name, size), is_gap))
        .child(
            div()
                .font_family(t.typography.mono_font.clone())
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(place),
        )
}

fn card(t: &Theme, label: &'static str, value: String) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .gap(t.spacing.xxs)
        .p(t.spacing.md)
        .rounded(t.radii.card)
        .border_1()
        .border_color(t.surfaces.card_edge)
        .child(
            div()
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(label),
        )
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .font_family(t.typography.numbers_font.clone())
                .child(value),
        )
}

fn line(t: &Theme, is_bad: bool, (title, detail): (String, Option<String>)) -> Div {
    let (glyph, color) = if is_bad {
        (IconName::TriangleAlert, t.colors.warn)
    } else {
        (IconName::Check, t.colors.ok)
    };
    div()
        .flex()
        .gap(t.spacing.sm)
        .child(div().pt(t.spacing.xxs).child(icon(glyph, px(14.), color)))
        .child(
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .child(div().text_color(color).child(title))
                .children(detail.map(|detail| {
                    div()
                        .text_size(t.typography.small)
                        .text_color(t.colors.muted)
                        .child(detail)
                })),
        )
}

fn checks(t: &Theme, ready: &Ready) -> Vec<Div> {
    let image = &ready.image;
    let mut lines: Vec<Div> = image
        .problems
        .iter()
        .take(SHOWN_PROBLEMS)
        .map(|found| {
            let (title, detail) = problem(found, ready.wide);
            line(t, true, (title, Some(detail)))
        })
        .collect();
    let more = image.problem_total.saturating_sub(SHOWN_PROBLEMS as u64);
    if more > 0 {
        lines.push(line(
            t,
            true,
            (format!("and {} more", format::bytes(more)), None),
        ));
    }
    if image.checksum_errors == 0 {
        lines.push(line(t, false, ("Every checksum matches".to_owned(), None)));
    }
    let end = image
        .start
        .map(|at| format!("start {}", start(at, ready.wide)));
    lines.push(if image.has_end {
        line(t, false, ("End record present".to_owned(), end))
    } else {
        line(t, true, ("End record missing".to_owned(), end))
    });
    lines
}

fn records(t: &Theme, image: &Image) -> Vec<Div> {
    (0..image.format.record_kinds())
        .filter(|kind| image.counts.get(*kind) > 0)
        .map(|kind| {
            div()
                .flex()
                .justify_between()
                .gap(t.spacing.md)
                .child(image.format.record_label(kind))
                .child(
                    div()
                        .font_family(t.typography.numbers_font.clone())
                        .child(format::bytes(image.counts.get(kind))),
                )
        })
        .collect()
}

fn section(t: &Theme, title: &'static str, rows: Vec<Div>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.sm)
        .child(caption(t, title))
        .children(rows)
}

impl MemoryView {
    fn navigator_list(&self, cx: &mut Context<Self>) -> VirtualList {
        let view = cx.weak_entity();
        let count = self.ready().map_or(0, |ready| ready.items.len());
        VirtualList::new(
            "memory-items",
            count,
            &self.nav_scroll,
            move |range, _, cx| {
                let Some(entity) = view.upgrade() else {
                    return Vec::new();
                };
                let t = tokens::current(cx).clone();
                let this = entity.read(cx);
                let Some(ready) = this.ready() else {
                    return Vec::new();
                };
                range
                    .map(|at| {
                        item_row(&t, ready, at, at == this.selected)
                            .on_click(pick(view.clone(), at))
                            .into_any_element()
                    })
                    .collect()
            },
        )
        .row_height(px(ITEM_HEIGHT))
        .label("Ranges and gaps")
    }

    fn navigator(&self, t: &Theme, window: &Window, cx: &mut Context<Self>) -> SideSheet {
        let this = cx.weak_entity();
        let count = self.ready().map_or_else(String::new, |ready| {
            let ranges = ready.image.ranges.len() as u64;
            format::count(ranges, "range", "ranges")
        });
        let title = div()
            .flex()
            .items_baseline()
            .gap(t.spacing.md)
            .child(div().font_weight(FontWeight::SEMIBOLD).child("Ranges"))
            .child(
                div()
                    .text_size(t.typography.small)
                    .text_color(t.colors.muted)
                    .child(count),
            );
        SideSheet::new(
            "memory-navigator",
            &self.navigator,
            frame(t, window, (NAVIGATOR_WIDTH, Side::Left)),
        )
        .label("Ranges")
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
        .hint(&["enter"], "Go to")
        .hint(&["escape"], "Close")
    }

    fn inspector_body(&self, t: &Theme, ready: &Ready) -> Stateful<Div> {
        let Some(item) = ready.items.get(self.selected) else {
            return div().id("memory-inspector-body");
        };
        let span = &item.span;
        let whole = whole(&ready.image);
        let size = span.end - span.start;
        let into = span.start.saturating_sub(whole.start);
        let position = format!(
            "Starts {} into {}",
            percent(into, whole.end - whole.start),
            super::text::span(&whole, ready.wide)
        );
        div()
            .id("memory-inspector-body")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .gap(t.spacing.xl)
            .p(t.spacing.xl)
            .child(
                div()
                    .flex()
                    .gap(t.spacing.md)
                    .child(card(t, "Size", format::size(size)))
                    .child(card(
                        t,
                        "Of the span",
                        percent(size, whole.end - whole.start),
                    )),
            )
            .child(section(
                t,
                "POSITION",
                vec![div().text_color(t.colors.muted).child(position)],
            ))
            .child(section(
                t,
                "RECORDS BY TYPE · WHOLE FILE",
                records(t, &ready.image),
            ))
            .child(section(t, "CHECKS", checks(t, ready)))
    }

    fn inspector(&self, t: &Theme, window: &Window, cx: &mut Context<Self>) -> Option<SideSheet> {
        let ready = self.ready()?;
        let this = cx.weak_entity();
        let range = ready
            .items
            .get(self.selected)
            .map(|item| span(&item.span, ready.wide));
        let title = div()
            .flex()
            .flex_col()
            .min_w_0()
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(entries::name(&ready.items, self.selected)),
            )
            .children(range.map(|range| {
                div()
                    .font_family(t.typography.mono_font.clone())
                    .text_size(t.typography.small)
                    .text_color(t.colors.muted)
                    .child(range)
            }));
        Some(
            SideSheet::new(
                "memory-inspector",
                &self.inspector,
                frame(t, window, (INSPECTOR_WIDTH, Side::Right)),
            )
            .label("Inspector")
            .header(title)
            .close("ctrl-i", move |_, cx| {
                let _ = this.update(cx, |view, cx| view.toggle_inspector(cx));
            })
            .body(self.inspector_body(t, ready)),
        )
    }

    pub(super) fn drawers(&self, t: &Theme, window: &Window, cx: &mut Context<Self>) -> Div {
        let navigator = self
            .navigator
            .read(cx)
            .is_shown(cx)
            .then(|| self.navigator(t, window, cx));
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
