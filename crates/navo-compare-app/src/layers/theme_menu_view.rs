//! Drawing and keys of the Theme picker (NC150, NC151, NC153): palette rows with swatches, mode
//! rows and the popover card, on Sidewise tokens only.

use gpui_kit::base::{ObservedElement, TestSupportExt as _, TestTextExt as _};
use gpui_kit::{AnimationExt, KeyDownEvent, ReadGlobal, Role, Stateful, transparent_black};
use sidewise_kit::prelude::*;
use sidewise_kit::tokens;

use super::{ITEMS, Pick, ThemeMenu};
use crate::layers::menu_parts::{Look, card_shadow, heading, mark, toggled};
use crate::motion;
use crate::theme::SidewiseTheme;
use crate::theme_data::palettes::{DIMS, ModeTokens, PALETTES};

impl ThemeMenu {
    /// Two 10 px dots, the palette's accent and surface in the current mode, with no label.
    fn swatch(ix: usize, look: &Look, dark: bool) -> impl IntoElement {
        let spec = &PALETTES[ix].1;
        let tokens = if dark { spec.dark } else { spec.light };
        let border = look.m.sep_strong;
        let dot = move |color| {
            div()
                .size(px(10.))
                .rounded_full()
                .border_1()
                .border_color(border)
                .bg(color)
        };
        div()
            .id(("theme-swatch", ix))
            .flex()
            .flex_none()
            .gap_1()
            .child(dot(tokens.accent))
            .child(dot(tokens.surface))
    }

    /// The "AAA" tag of the High contrast row, styled like a key cap but plain text.
    fn contrast_tag(cx: &App) -> impl IntoElement {
        sidewise_kit::assets::load_mono_fonts(cx);
        let t = tokens::current(cx);
        div()
            .id(("theme-key", 4usize))
            .role(Role::Label)
            .aria_label("AAA contrast")
            .test_support()
            .flex_none()
            .px_1()
            .font_family(t.typography.mono_font.clone())
            .text_size(t.typography.key)
            .font_weight(FontWeight::MEDIUM)
            .rounded(t.radii.sm)
            .border_1()
            .border_color(t.colors.line)
            .text_color(t.colors.muted)
            .child(div().test_text("AAA").child("AAA"))
    }

    /// One row at `ix`: mark, swatch (palettes), label and hint, ringed when keyboard-active.
    fn row(&self, ix: usize, look: &Look, cx: &mut Context<Self>) -> impl IntoElement {
        let (pick, label) = ITEMS[ix];
        let (on, m) = (self.is_on(pick), look.m);
        let dark = SidewiseTheme::global(cx).mode.is_dark();
        let ring = if look.focused && self.active == ix {
            m.accent
        } else {
            transparent_black()
        };
        div()
            .id(("theme-item", ix))
            .role(Role::MenuItemRadio)
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
            .child(mark(on, false, &m))
            .when(matches!(pick, Pick::Palette(_)), |el| {
                el.child(Self::swatch(ix, look, dark))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(label),
            )
            .when(ix == 4, |el| el.child(Self::contrast_tag(cx)))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.active = ix;
                this.apply(pick, cx);
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

    /// The card: palettes, a rule, then modes.
    fn card(&self, look: &Look, cx: &mut Context<Self>) -> ObservedElement<Stateful<Div>> {
        let m: ModeTokens = look.m;
        div()
            .id("theme-menu")
            .role(Role::Menu)
            .aria_label("Colour theme")
            .test_support()
            .key_context("ThemeMenu")
            .track_focus(&self.handle)
            .occlude()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| this.key(event, cx)))
            .flex()
            .flex_col()
            .w(px(280.))
            .p_1()
            .rounded(DIMS.card_r)
            .border_1()
            .border_color(m.sep)
            .bg(m.card)
            .text_color(m.text)
            .text_size(DIMS.fs)
            .shadow(vec![card_shadow(&m)])
            .child(heading("Colour theme", &m))
            .children(self.rows(0..5, look, cx))
            .child(div().h(px(1.)).mx_2().my_1().bg(m.sep))
            .child(heading("Mode", &m))
            .children(self.rows(5..8, look, cx))
    }
}

impl Render for ThemeMenu {
    /// The popover on the solid `card` surface; it fades in through the menu token (opacity only).
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let side = *SidewiseTheme::global(cx);
        let dark = side.mode.is_dark();
        let m = if dark {
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
        let enter = motion::anim("theme-menu", side.motion.menu_ms, cx);
        self.animated.set(enter.is_some());
        match enter {
            Some(enter) => card
                .with_animation("theme-menu", enter, |el, delta| el.opacity(delta))
                .into_any_element(),
            None => card.into_any_element(),
        }
    }
}
