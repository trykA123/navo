//! Drawing of the View menu (U16b): the segmented presets, the entry rows and the popover card,
//! on Sidewise tokens only.

use gpui_kit::base::{ObservedElement, TestSupportExt as _};
use gpui_kit::{AnimationExt, KeyDownEvent, ReadGlobal, Role, Stateful, transparent_black};
use sidewise_kit::prelude::*;
use sidewise_kit::primitives::Kbd;

use super::{Act, ITEMS, ViewMenu};
use crate::layers::menu_parts::{Look, card_shadow, heading, mark, toggled};
use crate::motion;
use crate::theme::SidewiseTheme;
use crate::theme_data::palettes::{DIMS, ModeTokens};

impl ViewMenu {
    /// One entry at `ix`: mark, label and key hint, with a focus ring when it is keyboard-active.
    fn row(&self, ix: usize, look: &Look, cx: &mut Context<Self>) -> impl IntoElement {
        let (act, label, key) = ITEMS[ix];
        let (on, check) = (self.is_on(act), matches!(act, Act::Toggle(_)));
        let m = look.m;
        let ring = if look.focused && self.active == ix {
            m.accent
        } else {
            transparent_black()
        };
        div()
            .id(("view-item", ix))
            .role(if check {
                Role::MenuItemCheckBox
            } else {
                Role::MenuItemRadio
            })
            .aria_label(label)
            .aria_toggled(toggled(on))
            .test_support()
            .flex()
            .items_center()
            .gap_2()
            .min_h(px(34.))
            .px_2()
            .rounded(DIMS.btn_r)
            .cursor_pointer()
            .border(look.ring_w)
            .border_color(ring)
            .hover(move |style| style.bg(m.accent_tint))
            .child(mark(on, check, &m))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(label),
            )
            .children(
                (!key.is_empty())
                    .then(|| Kbd::new(("view-key", ix), key))
                    .flatten(),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.active = ix;
                this.apply(act, cx);
            }))
    }

    /// The Simple/Rich/Custom segmented control: a `layer2` track with the chosen thumb on `card`.
    fn presets(&self, m: &ModeTokens, cx: &mut Context<Self>) -> impl IntoElement {
        let m = *m;
        div()
            .flex()
            .gap_0p5()
            .p(px(2.))
            .rounded(DIMS.btn_r)
            .bg(m.layer2)
            .children((0..3).map(|ix| {
                let (act, label, _) = ITEMS[ix];
                let on = self.is_on(act);
                div()
                    .id(("view-item", ix))
                    .role(Role::Button)
                    .aria_label(label)
                    .aria_toggled(toggled(on))
                    .test_support()
                    .px_3()
                    .min_h(px(28.))
                    .flex()
                    .items_center()
                    .rounded(px(8.))
                    .cursor_pointer()
                    .text_color(if on { m.text } else { m.muted })
                    .when(on, |el| el.bg(m.card).font_weight(FontWeight::SEMIBOLD))
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.active = ix;
                        this.apply(act, cx);
                    }))
            }))
    }

    /// The rows `range` of `ITEMS`, in order.
    fn rows(
        &self,
        range: std::ops::Range<usize>,
        look: &Look,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        range
            .map(|ix| self.row(ix, look, cx).into_any_element())
            .collect()
    }

    /// The card: presets, layouts and toggles, in the prototype's order.
    fn card(&self, look: &Look, cx: &mut Context<Self>) -> ObservedElement<Stateful<Div>> {
        let m = look.m;
        let rule = || div().h(px(1.)).mx_2().my_1().bg(m.sep);
        div()
            .id("view-menu")
            .role(Role::Menu)
            .aria_label("View")
            .test_support()
            .key_context("ViewMenu")
            .track_focus(&self.handle)
            .occlude()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| this.key(event, cx)))
            .flex()
            .flex_col()
            .w(px(320.))
            .p_1()
            .rounded(DIMS.card_r)
            .border_1()
            .border_color(m.sep)
            .bg(m.card)
            .text_color(m.text)
            .text_size(DIMS.fs)
            .shadow(vec![card_shadow(&m)])
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(heading("View", &m))
                    .child(self.presets(&m, cx)),
            )
            .children(self.rows(3..5, look, cx))
            .child(rule())
            .children(self.rows(5..7, look, cx))
    }
}

impl Render for ViewMenu {
    /// The popover on the solid `card` surface with a hairline and shadow; it fades in through the menu token.
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let side = *SidewiseTheme::global(cx);
        let m = if side.mode.is_dark() {
            side.spec.dark
        } else {
            side.spec.light
        };
        let look = Look {
            m,
            ring_w: px(side.spec.ring_px),
            focused: self.handle.is_focused(window),
        };
        let card = self.card(&look, cx);
        let enter = motion::anim("view-menu", side.motion.menu_ms, cx);
        self.animated.set(enter.is_some());
        match enter {
            Some(enter) => card
                .with_animation("view-menu", enter, |el, delta| el.opacity(delta))
                .into_any_element(),
            None => card.into_any_element(),
        }
    }
}
