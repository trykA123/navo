//! Drawing of the actions menu and the status line.

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, Context, Div, ElementId, InteractiveElement, IntoElement, ParentElement, Role,
    SharedString, StatefulInteractiveElement, Styled, div, px,
};
use navo_l10n::{t, tr};

use super::actions::{Item, items};
use super::status::{Status, Tone};
use super::view::{ClipPalette, LIST_WIDTH};
use crate::primitives::{IconName, Kbd, icon};
use crate::tokens::Theme;

const ITEM: f32 = 26.;
const MENU_WIDTH: f32 = 248.;
const FOOTER: f32 = 28.;
const TITLE_CHARS: usize = 60;

/// Fades an element in over the theme's fade time; plain when motion is reduced.
pub(super) fn fade_in<E>(element: E, id: impl Into<ElementId>, t: &Theme, cx: &App) -> AnyElement
where
    E: IntoElement + Styled + 'static,
{
    use gpui_kit::base::animation::ease_out_cubic;
    use gpui_kit::{Animation, AnimationExt};
    if cx.reduce_motion() {
        return element.into_any_element();
    }
    element
        .with_animation(
            id,
            Animation::new(t.motion.fade).with_easing(ease_out_cubic),
            |element, delta| element.opacity(delta),
        )
        .into_any_element()
}

fn heading(t: &Theme, title: &str) -> Div {
    let short: String = title.chars().take(TITLE_CHARS).collect();
    div()
        .px(t.spacing.md)
        .pt(t.spacing.xs)
        .pb(t.spacing.sm)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .truncate()
        .child(SharedString::from(short))
}

fn menu_item(
    t: &Theme,
    (index, item): (usize, Item),
    is_selected: bool,
    cx: &mut Context<ClipPalette>,
) -> impl IntoElement {
    let theme = t.selected_content(is_selected);
    let t = &*theme;
    let label = SharedString::from(item.label);
    div()
        .id(("clip-palette-action", index))
        .role(Role::MenuItem)
        .aria_label(label.clone())
        .aria_selected(is_selected)
        .test_support()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .h(px(ITEM))
        .px(t.spacing.md)
        .rounded(t.radii.sm)
        .cursor_pointer()
        .when(is_selected, |row| {
            row.bg(t.colors.selection).text_color(t.colors.on_selection)
        })
        .when(!is_selected, |row| row.hover(|row| row.bg(t.colors.hover)))
        .child(div().flex_1().min_w_0().truncate().child(label))
        .children(Kbd::new(("clip-palette-action-key", index), item.key))
        .on_mouse_move(cx.listener(move |palette, _, _, cx| palette.point_at_action(index, cx)))
        .on_click(cx.listener(move |palette, _, _, cx| {
            palette.point_at_action(index, cx);
            palette.choose(cx);
        }))
}

fn countdown(t: &Theme, secs: u32) -> impl IntoElement {
    let text = t!("clip-status-clears", secs = secs);
    div()
        .id("clip-palette-status-clears")
        .aria_label(text.clone())
        .test_support()
        .flex_none()
        .text_color(t.colors.muted)
        .font_features(t.typography.numbers_font_features.clone())
        .child(text)
}

impl ClipPalette {
    /// A click catcher over the palette and the menu above it; empty while the menu is shut.
    pub(super) fn actions_menu(&self, t: &Theme, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let (Some(at), Some(row)) = (self.overlay.menu, self.state.selected_row()) else {
            return Vec::new();
        };
        let lines: Vec<AnyElement> = items(row, self.facts(row))
            .into_iter()
            .enumerate()
            .map(|(index, item)| menu_item(t, (index, item), index == at, cx).into_any_element())
            .collect();
        let backdrop = div()
            .id("clip-palette-menu-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .on_click(cx.listener(|palette, _, _, cx| {
                palette.close_actions(cx);
            }));
        let menu = div()
            .id("clip-palette-menu")
            .role(Role::Menu)
            .aria_label(tr("clip-act-actions"))
            .test_support()
            .occlude()
            .absolute()
            .right(t.spacing.md)
            .bottom(px(FOOTER) + t.spacing.xs)
            .w(px(MENU_WIDTH))
            .max_w_full()
            .p(t.spacing.xs)
            .rounded(t.radii.lg)
            .border_1()
            .border_color(t.colors.line)
            .bg(t.colors.surface)
            .shadow(t.shadows.menu.clone())
            .child(heading(t, &row.title))
            .children(lines);
        vec![
            backdrop.into_any_element(),
            fade_in(menu, "clip-palette-menu-in", t, cx),
        ]
    }

    /// The status line, centred above the footer; none until an action has run.
    /// The status floats over the preview, clear of the list, as the prototype's toast does
    /// outside the palette.
    pub(super) fn status_line(&self, t: &Theme, is_docked: bool, cx: &App) -> Option<AnyElement> {
        let Status {
            text,
            tone,
            serial,
            left,
        } = self.overlay.status.as_ref()?;
        let (glyph, color) = match tone {
            Tone::Done => (IconName::Check, t.colors.ok),
            Tone::Error => (IconName::TriangleAlert, t.colors.warn),
            Tone::Secret => (IconName::Shield, t.colors.warn),
        };
        let line = div()
            .id("clip-palette-status")
            .role(Role::Status)
            .aria_label(text.clone())
            .test_support()
            .absolute()
            .bottom(px(FOOTER) + t.spacing.xl)
            .left(
                if is_docked || self.manage.is_some() || self.form.is_some() {
                    px(0.)
                } else {
                    px(LIST_WIDTH)
                },
            )
            .right_0()
            .px(t.spacing.md)
            .flex()
            .justify_center()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(t.spacing.md)
                    .max_w_full()
                    .px(t.spacing.xl)
                    .py(t.spacing.md)
                    .rounded(t.radii.lg)
                    .border_1()
                    .border_color(t.colors.line)
                    .bg(t.colors.surface)
                    .shadow(t.shadows.menu.clone())
                    .text_size(t.typography.small)
                    .child(icon(glyph, t.typography.small, color))
                    .child(div().min_w_0().truncate().child(text.clone()))
                    .children(left.map(|secs| countdown(t, secs))),
            );
        Some(fade_in(line, ("clip-palette-status-in", *serial), t, cx))
    }
}
