use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;
use rusty_automotive::{Elf, Symbol};
use rusty_l10n::{t, tr};

use super::rich::block_name;
use super::text::{Check, is_wide, span};
use super::{ElfView, Index};
use crate::format;
use crate::prelude::*;
use crate::primitives::{IconName, icon};
use crate::tokens::Theme;
use crate::window_view::viewer::space::{
    Pick, Picked, pick_facts, share_cell, share_head, share_of,
};

const CHECK_SIZE: f32 = 16.;

impl ElfView {
    pub(super) fn shows_share(&self) -> bool {
        self.is_rich() && self.layout.row_bars
    }

    pub(super) fn share_head(&self, t: &Theme) -> Option<Div> {
        self.shows_share().then(|| share_head(t))
    }

    pub(super) fn share_cell(&self, t: &Theme, symbol: &Symbol) -> Option<Div> {
        if !self.shows_share() {
            return None;
        }
        let share = self.rich.model.as_ref().and_then(|model| {
            let estimate = self.elf().and_then(|elf| {
                let boundaries = model.boundaries.as_ref()?;
                boundaries.estimated_size(elf, symbol)
            });
            share_of(&model.regions, (symbol.value, symbol.size), estimate)
        });
        Some(share_cell(t, share))
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
        let total = self.hidden.saturating_add(self.rows.len() as u64);
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
        let outside = self
            .rich
            .model
            .as_ref()
            .and_then(|model| model.outside_note())
            .map(|text| Check { text, is_bad: true });
        (picked, outside)
    }

    pub(super) fn pick_body(&self, t: &Theme, elf: &Elf) -> Option<Vec<AnyElement>> {
        let picked = Picked {
            model: self.rich.model.as_ref()?,
            pick: self.rich.pick?,
            name: self.pick_name()?,
            name_id: "elf-inspector-name",
        };
        let wide = is_wide(elf);
        pick_facts(t, picked, &|start, size| span(start, size, wide))
    }
}
