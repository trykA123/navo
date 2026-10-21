use gpui_kit::base::TestSupportExt;
use gpui_kit::relative;
use rusty_automotive::space::{Block, Group, Kind, Region};
use rusty_l10n::{t, tr};

use super::model::RichModel;
use super::rich::Pick;
use super::strip::kind_name;
use crate::format;
use crate::prelude::*;
use crate::tokens::Theme;

const SHARE_WIDTH: f32 = 104.;
const LARGEST_ITEMS: usize = 8;
const KINDS: [Kind; 6] = [
    Kind::Code,
    Kind::Rodata,
    Kind::Data,
    Kind::Bss,
    Kind::Other,
    Kind::Fill,
];

pub(in crate::window_view::viewer) type SpanText<'a> = &'a dyn Fn(u64, u64) -> String;

pub(in crate::window_view::viewer) fn share_of(
    regions: &[std::sync::Arc<Region>],
    (address, size): (u64, u64),
) -> Option<f64> {
    let region = regions
        .iter()
        .find(|region| region.span().is_some_and(|span| span.contains(&address)))?;
    (region.used > 0).then(|| size as f64 / region.used as f64)
}

pub(in crate::window_view::viewer) fn share_head(t: &Theme) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_end()
        .w(px(SHARE_WIDTH))
        .pl(t.spacing.md)
        .pr(t.spacing.xl)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(tr("elf-column-share"))
}

pub(in crate::window_view::viewer) fn share_cell(t: &Theme, share: Option<f64>) -> Div {
    let text = share.map_or_else(String::new, |share| {
        t!(
            "viewer-space-share",
            percent = format!("{:.1}", share * 100.)
        )
    });
    let bar = div()
        .w(px(28.))
        .h(px(6.))
        .flex_none()
        .rounded(t.radii.xs)
        .bg(t.mem.free)
        .child(
            div()
                .h_full()
                .w(relative(share.unwrap_or(0.).clamp(0., 1.) as f32))
                .bg(t.mem.code.strong),
        );
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_end()
        .gap(t.spacing.sm)
        .w(px(SHARE_WIDTH))
        .pl(t.spacing.md)
        .pr(t.spacing.xl)
        .font_family(t.typography.numbers_font.clone())
        .text_size(t.typography.small)
        .child(bar)
        .child(text)
}

fn fact(t: &Theme, label: &str, value: String) -> Div {
    div()
        .flex()
        .justify_between()
        .gap(t.spacing.md)
        .child(
            div()
                .flex_none()
                .text_color(t.colors.muted)
                .child(tr(label)),
        )
        .child(div().min_w_0().text_right().child(value))
}

fn caption(t: &Theme, text: &str) -> Div {
    div()
        .text_size(t.typography.tiny)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(t.colors.muted)
        .child(tr(text))
}

fn full_name(id: &'static str, name: String) -> impl IntoElement {
    div()
        .id(id)
        .aria_label(name.clone())
        .test_support()
        .font_weight(FontWeight::SEMIBOLD)
        .child(name)
}

fn line(t: &Theme, left: String, right: String) -> AnyElement {
    div()
        .flex()
        .justify_between()
        .gap(t.spacing.md)
        .child(div().min_w_0().text_color(t.colors.muted).child(left))
        .child(div().flex_none().child(right))
        .into_any_element()
}

fn block_facts(
    t: &Theme,
    (region, block): (&Region, &Block),
    span: SpanText<'_>,
) -> Vec<AnyElement> {
    let facts = [
        ("space-inspector-region", region.name.clone()),
        ("space-inspector-kind", kind_name(block.kind)),
        ("space-inspector-address", span(block.start, block.size)),
        ("space-inspector-size", format::exact_bytes(block.size)),
    ];
    std::iter::once(caption(t, "space-inspector-block").into_any_element())
        .chain(
            facts
                .into_iter()
                .map(|(label, value)| fact(t, label, value).into_any_element()),
        )
        .collect()
}

fn group_facts(t: &Theme, group: &Group) -> Vec<AnyElement> {
    let kinds = KINDS
        .iter()
        .filter(|kind| group.by_kind[kind.index()] > 0)
        .map(|kind| {
            line(
                t,
                kind_name(*kind),
                format::size(group.by_kind[kind.index()]),
            )
        });
    let largest = group
        .children
        .iter()
        .take(LARGEST_ITEMS)
        .map(|child| line(t, child.name.clone(), format::size(child.bytes)));
    let head = [
        caption(t, "space-inspector-group").into_any_element(),
        fact(t, "space-inspector-size", format::exact_bytes(group.bytes)).into_any_element(),
        fact(
            t,
            "space-inspector-items",
            format::bytes(u64::from(group.items)),
        )
        .into_any_element(),
    ];
    head.into_iter()
        .chain(kinds)
        .chain(std::iter::once(
            caption(t, "space-inspector-largest").into_any_element(),
        ))
        .chain(largest)
        .collect()
}

pub(in crate::window_view::viewer) struct Picked<'a> {
    pub model: &'a RichModel,
    pub pick: Pick,
    pub name: String,
    pub name_id: &'static str,
}

pub(in crate::window_view::viewer) fn pick_facts(
    t: &Theme,
    picked: Picked<'_>,
    span: SpanText<'_>,
) -> Option<Vec<AnyElement>> {
    let Picked {
        model,
        pick,
        name,
        name_id,
    } = picked;
    let rest = match pick {
        Pick::Block { region, block } => {
            let region = model.regions.get(region)?;
            block_facts(t, (region, region.blocks.get(block)?), span)
        }
        Pick::Group(id) => group_facts(t, model.groups.get(id as usize)?),
    };
    Some(
        std::iter::once(full_name(name_id, name).into_any_element())
            .chain(rest)
            .collect(),
    )
}
