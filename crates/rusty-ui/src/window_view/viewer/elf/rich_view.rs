use gpui_kit::base::TestSupportExt;
use gpui_kit::{Role, relative};
use rusty_automotive::space::{Block, Group, Kind, Region};
use rusty_automotive::{Elf, Symbol};
use rusty_l10n::{t, tr};

use super::rich::block_name;
use super::text::{Check, is_wide, span};
use super::{ElfView, Index};
use crate::format;
use crate::prelude::*;
use crate::primitives::{IconName, icon};
use crate::tokens::Theme;
use crate::window_view::viewer::space::{Pick, kind_name};

const SHARE_WIDTH: f32 = 96.;
const CHECK_SIZE: f32 = 16.;
const LARGEST_ITEMS: usize = 8;
const KINDS: [Kind; 6] = [
    Kind::Code,
    Kind::Rodata,
    Kind::Data,
    Kind::Bss,
    Kind::Other,
    Kind::Fill,
];

fn share_of(regions: &[std::sync::Arc<Region>], symbol: &Symbol) -> Option<f64> {
    let region = regions.iter().find(|region| {
        region
            .span()
            .is_some_and(|span| span.contains(&symbol.value))
    })?;
    (region.used > 0).then(|| symbol.size as f64 / region.used as f64)
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

fn full_name(name: String) -> impl IntoElement {
    div()
        .id("elf-inspector-name")
        .aria_label(name.clone())
        .test_support()
        .font_weight(FontWeight::SEMIBOLD)
        .child(name)
}

impl ElfView {
    pub(super) fn shows_share(&self) -> bool {
        self.is_rich() && self.layout.row_bars
    }

    pub(super) fn share_head(&self, t: &Theme) -> Option<Div> {
        self.shows_share().then(|| {
            div()
                .flex()
                .flex_none()
                .items_center()
                .justify_end()
                .w(px(SHARE_WIDTH))
                .px(t.spacing.md)
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(tr("elf-column-share"))
        })
    }

    pub(super) fn share_cell(&self, t: &Theme, symbol: &Symbol) -> Option<Div> {
        if !self.shows_share() {
            return None;
        }
        let share = self
            .rich
            .model
            .as_ref()
            .and_then(|model| share_of(&model.regions, symbol));
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
        Some(
            div()
                .flex()
                .flex_none()
                .items_center()
                .justify_end()
                .gap(t.spacing.sm)
                .w(px(SHARE_WIDTH))
                .px(t.spacing.md)
                .font_family(t.typography.numbers_font.clone())
                .text_size(t.typography.small)
                .child(bar)
                .child(text),
        )
    }

    fn noise_check(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let is_on = self.query.hide_noise;
        let mark = div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(CHECK_SIZE))
            .rounded(t.radii.xs)
            .border_1()
            .border_color(t.colors.faint)
            .when(is_on, |mark| {
                mark.child(icon(IconName::Check, px(12.), t.colors.accent_text))
            });
        div()
            .id("elf-noise")
            .role(Role::CheckBox)
            .aria_label(tr("elf-noise-check"))
            .aria_selected(is_on)
            .test_support()
            .flex()
            .items_center()
            .gap(t.spacing.sm)
            .cursor_pointer()
            .child(mark)
            .child(tr("elf-noise-check"))
            .on_click(cx.listener(|view, _, window, cx| {
                window.focus(&view.focus, cx);
                view.toggle_noise(cx);
            }))
    }

    pub(super) fn noise_bar(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let total = self.elf().map_or(0, |elf| elf.symbols.len() as u64);
        let hidden = t!(
            "elf-noise-hidden",
            hidden = format::bytes(self.hidden),
            total = format::bytes(total)
        );
        let shown = t!(
            "elf-noise-shown",
            count = format::bytes(self.rows.len() as u64)
        );
        div()
            .id("elf-noise-bar")
            .aria_label(format!("{hidden} · {shown}"))
            .test_support()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.lg)
            .h(t.spacing.row_height)
            .px(t.spacing.md)
            .text_size(t.typography.small)
            .child(self.noise_check(t, cx))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(t.colors.muted)
                    .child(hidden),
            )
            .child(div().flex_none().text_color(t.colors.muted).child(shown))
    }

    pub(super) fn pick_name(&self) -> Option<String> {
        let (Index::Ready(elf), Some(model)) = (&self.index, &self.rich.model) else {
            return None;
        };
        match self.rich.pick? {
            Pick::Block { region, block } => {
                Some(block_name(elf, model.regions.get(region)?, block))
            }
            Pick::Group(id) => Some(model.groups.get(id as usize)?.name.clone()),
        }
    }

    pub(super) fn rich_checks(&self) -> (Option<Check>, Option<Check>) {
        let picked = self
            .pick_name()
            .filter(|_| self.is_rich())
            .map(|text| Check {
                text,
                is_bad: false,
            });
        let outside = self.rich.model.as_ref().and_then(|model| {
            let name = model.region_source.clone()?;
            (model.outside > 0).then(|| Check {
                text: t!("space-outside", count = model.outside as i64, name = name),
                is_bad: true,
            })
        });
        (picked, outside)
    }

    pub(super) fn pick_body(&self, t: &Theme, elf: &Elf) -> Option<Vec<AnyElement>> {
        let model = self.rich.model.as_ref()?;
        let name = self.pick_name()?;
        let rest = match self.rich.pick? {
            Pick::Block { region, block } => {
                let region = model.regions.get(region)?;
                block_facts(t, region, region.blocks.get(block)?, is_wide(elf))
            }
            Pick::Group(id) => group_facts(t, model.groups.get(id as usize)?),
        };
        Some(
            std::iter::once(full_name(name).into_any_element())
                .chain(rest)
                .collect(),
        )
    }
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

fn block_facts(t: &Theme, region: &Region, block: &Block, wide: bool) -> Vec<AnyElement> {
    let facts = [
        ("space-inspector-region", region.name.clone()),
        ("space-inspector-kind", kind_name(block.kind)),
        (
            "space-inspector-address",
            span(block.start, block.size, wide),
        ),
        ("space-inspector-size", format::size(block.size)),
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
        fact(t, "space-inspector-size", format::size(group.bytes)).into_any_element(),
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
