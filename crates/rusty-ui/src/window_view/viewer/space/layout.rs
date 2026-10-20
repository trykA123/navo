use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::{Role, Stateful, WeakEntity, canvas, relative};
use rusty_automotive::space::{Grouping, Kind, Region, ScaleMode};
use rusty_l10n::{t, tr};

use super::bars::{Legend, RegionHead, UsageBar, UsageLine};
use super::model::RichModel;
use super::rich::{Pick, RichHost, RichSpace};
use super::switch::{Choice, switch};
use super::treemap::{Treemap, TreemapEvent};
use crate::format;
use crate::prelude::*;
use crate::tokens::Theme;
use crate::window_view::viewer::parts::LayoutParts;

const HEAD_WIDTH: f32 = 200.;
const WHO_SHARE: f32 = 5. / 12.;
const STACKED_WHO: f32 = 0.4;
const WIDE_CONTENT: f32 = 1100.;
const TOP_SHARE: f32 = 0.55;

const GROUPINGS: [(Grouping, &str); 4] = [
    (Grouping::Module, "space-group-module"),
    (Grouping::SourceFile, "space-group-source"),
    (Grouping::ObjectFile, "space-group-object"),
    (Grouping::Section, "space-group-section"),
];

pub(in crate::window_view::viewer) struct RichLook {
    pub parts: LayoutParts,
    pub width: Pixels,
    pub file_bytes: Option<AnyElement>,
    pub table: AnyElement,
}

impl LayoutParts {
    pub(in crate::window_view::viewer) fn is_rich(self) -> bool {
        self.strip || self.bars || self.who
    }
}

fn provenance(t: &Theme, text: String) -> impl IntoElement {
    div()
        .id("space-provenance")
        .aria_label(text.clone())
        .test_support()
        .flex_none()
        .px(t.spacing.xl)
        .py(t.spacing.xs)
        .truncate()
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(text)
}

fn scale_switch<V: RichHost>(t: &Theme, mode: ScaleMode, view: WeakEntity<V>) -> Div {
    let choices = [
        (ScaleMode::Compressed, "space-scale-compressed"),
        (ScaleMode::True, "space-scale-true"),
    ];
    let options = choices
        .iter()
        .map(|(shown, text)| Choice {
            label: tr(text),
            is_on: *shown == mode,
            blocked: None,
        })
        .collect();
    switch(
        t,
        "space-scale",
        options,
        Rc::new(move |at, _, cx| {
            let mode = choices[at].0;
            let _ = view.update(cx, |view, cx| {
                view.rich().set_mode(mode, cx);
                cx.notify();
            });
        }),
    )
}

fn region_row(
    t: &Theme,
    (at, region): (usize, &Arc<Region>),
    rich: &RichSpace,
    look: &RichLook,
) -> Stateful<Div> {
    let parts = look.parts;
    let is_wide = look.width >= px(WIDE_CONTENT);
    let head = if parts.bars {
        div().child(RegionHead::new(region))
    } else {
        div()
            .truncate()
            .text_size(t.typography.small)
            .font_weight(FontWeight::SEMIBOLD)
            .child(region.name.clone())
    };
    let strip = rich.strips.get(at).filter(|_| parts.strip).cloned();
    let bars = parts.bars.then(|| {
        div()
            .flex()
            .flex_col()
            .gap(t.spacing.xxs)
            .child(UsageBar::new(("space-usage", at), region))
            .child(UsageLine::new(region, None))
    });
    let right = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .gap(t.spacing.xs)
        .children(bars)
        .children(strip);
    let row = div().id(("space-region", at)).flex().gap(t.spacing.xl);
    if is_wide {
        row.child(div().w(px(HEAD_WIDTH)).flex_none().child(head))
            .child(right)
    } else {
        row.flex_col().gap(t.spacing.xs).child(head).child(right)
    }
}

fn panel<V: RichHost>(
    t: &Theme,
    (rich, model): (&RichSpace, &RichModel),
    look: &RichLook,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    let fill = model
        .regions
        .iter()
        .any(|region| region.by_kind[Kind::Fill.index()] > 0);
    let header = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(t.spacing.lg)
        .child(
            div()
                .text_size(t.typography.small)
                .font_weight(FontWeight::SEMIBOLD)
                .child(tr("space-title")),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(Legend::new().with_fill(fill)),
        )
        .when(look.parts.strip, |header| {
            header.child(scale_switch(t, rich.mode, cx.weak_entity()))
        });
    let rows = model
        .regions
        .iter()
        .enumerate()
        .map(|row| region_row(t, row, rich, look));
    div()
        .id("space-panel")
        .flex()
        .flex_col()
        .flex_none()
        .gap(t.spacing.sm)
        .px(t.spacing.xl)
        .py(t.spacing.sm)
        .border_b_1()
        .border_color(t.colors.line)
        .child(header)
        .children(rows)
}

fn blocked(model: &RichModel, at: usize) -> Option<String> {
    if model.usable[at] {
        return None;
    }
    Some(tr(match GROUPINGS[at].0 {
        Grouping::SourceFile => "space-group-no-source",
        Grouping::ObjectFile => "space-group-no-object",
        Grouping::Module | Grouping::Section => "space-group-no-owners",
    }))
}

fn grouping_switch<V: RichHost>(t: &Theme, model: &RichModel, view: WeakEntity<V>) -> Div {
    let options = GROUPINGS
        .iter()
        .enumerate()
        .map(|(at, (grouping, text))| Choice {
            label: tr(text),
            is_on: *grouping == model.grouping,
            blocked: blocked(model, at),
        })
        .collect();
    switch(
        t,
        "space-grouping",
        options,
        Rc::new(move |at, _, cx| {
            let _ = view.update(cx, |view, cx| view.regroup(GROUPINGS[at].0, cx));
        }),
    )
}

fn treemap_box<V: RichHost>(rich: &RichSpace, model: &RichModel, cx: &mut Context<V>) -> Div {
    let measure = cx.weak_entity();
    let ruler = canvas(
        move |bounds, _, cx| {
            let _ = measure.update(cx, |view, cx| {
                if view.rich().measure_treemap(bounds) {
                    cx.notify();
                }
            });
        },
        |_, (), _, _| {},
    )
    .absolute()
    .size_full();
    let stripped = !model.owners.provenance.has_symbols;
    let selected = match rich.pick {
        Some(Pick::Group(id)) => Some(id),
        _ => None,
    };
    let picker = cx.weak_entity();
    let map = rich.tiles().filter(|_| !stripped).map(|tiles| {
        Treemap::new("space-treemap", rich.groups(), tiles)
            .selected(selected)
            .on_event(move |TreemapEvent::Pick(id), _, cx| {
                let _ = picker.update(cx, |view, cx| view.picked(Pick::Group(id), cx));
            })
    });
    let note = stripped
        .then(|| Treemap::new("space-treemap", rich.groups(), Rc::from(Vec::new())).stripped(true));
    div()
        .relative()
        .flex_1()
        .min_h_0()
        .child(ruler)
        .children(map)
        .children(note)
}

fn who_panel<V: RichHost>(
    t: &Theme,
    (rich, model): (&RichSpace, &RichModel),
    cx: &mut Context<V>,
) -> ObservedElement<Stateful<Div>> {
    let hint = model
        .flash
        .and_then(|at| model.regions.get(at))
        .map(|region| {
            t!(
                "space-who-hint",
                region = region.name.clone(),
                bytes = format::size(region.used)
            )
        });
    let title = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(t.spacing.md)
        .child(
            div()
                .text_size(t.typography.small)
                .font_weight(FontWeight::SEMIBOLD)
                .child(tr("space-who-title")),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(hint.unwrap_or_else(|| tr("space-who-none"))),
        )
        .child(grouping_switch(t, model, cx.weak_entity()));
    div()
        .id("space-who")
        .role(Role::Group)
        .aria_label(tr("space-who-title"))
        .test_support()
        .flex()
        .flex_col()
        .gap(t.spacing.sm)
        .p(t.spacing.md)
        .min_h_0()
        .child(title)
        .child(treemap_box(rich, model, cx))
}

fn body<V: RichHost>(
    t: &Theme,
    (rich, model): (&RichSpace, &RichModel),
    look: RichLook,
    cx: &mut Context<V>,
) -> Div {
    let table = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .child(look.table);
    let root = div().flex().flex_1().min_h_0();
    if !look.parts.who {
        return root.child(table);
    }
    let who = who_panel(t, (rich, model), cx);
    if look.width >= px(WIDE_CONTENT) {
        root.child(
            who.w(relative(WHO_SHARE))
                .flex_none()
                .border_r_1()
                .border_color(t.colors.line),
        )
        .child(table)
    } else {
        root.flex_col()
            .child(
                who.h(relative(STACKED_WHO))
                    .flex_none()
                    .border_b_1()
                    .border_color(t.colors.line),
            )
            .child(table)
    }
}

pub(in crate::window_view::viewer) fn rich_layout<V: RichHost>(
    t: &Theme,
    rich: &RichSpace,
    mut look: RichLook,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    let root = div().id("space-rich").flex().flex_col().flex_1().min_h_0();
    let Some(model) = rich.model.clone() else {
        return root
            .child(provenance(t, tr("space-loading")))
            .child(look.table);
    };
    let shows_panel = look.parts.strip || look.parts.bars;
    let file_bytes = look
        .file_bytes
        .take()
        .filter(|_| look.parts.bars)
        .map(|bar| {
            div()
                .flex_none()
                .px(t.spacing.xl)
                .py(t.spacing.sm)
                .border_b_1()
                .border_color(t.colors.line)
                .child(bar)
        });
    let top = div()
        .id("space-top")
        .flex()
        .flex_col()
        .flex_shrink_0()
        .max_h(relative(TOP_SHARE))
        .overflow_y_scroll()
        .when(shows_panel, |top| {
            top.child(panel(t, (rich, &model), &look, cx))
        })
        .children(file_bytes);
    root.child(provenance(t, model.provenance.clone()))
        .child(top)
        .child(body(t, (rich, &model), look, cx))
}
