use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;
use gpui_kit::component::Selectable;
use gpui_kit::component::button::Button as KitButton;
use rusty_core::{Command, Layout, PaneId};

use super::FerriteWindow;
use super::pane::title_of;
use crate::prelude::*;
use crate::tokens::{self, Theme};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tag {
    Active,
    Target,
}

struct StripPane {
    n: usize,
    name: SharedString,
    tag: Option<Tag>,
}

fn thumbnail(t: &Theme, layout: &Layout, focused: PaneId, target: Option<PaneId>) -> Div {
    let cells = layout.rects().into_iter().map(|(pane, rect)| {
        let cell = div()
            .size_full()
            .rounded(t.radii.xs)
            .border_1()
            .border_color(t.colors.line);
        let cell = match pane {
            pane if pane == focused => cell.bg(t.colors.accent).border_color(t.colors.accent),
            pane if Some(pane) == target => cell.border_color(t.colors.muted),
            _ => cell,
        };
        div()
            .absolute()
            .left(relative(rect.x as f32))
            .top(relative(rect.y as f32))
            .w(relative(rect.width as f32))
            .h(relative(rect.height as f32))
            .p(t.spacing.xxs / 2.)
            .child(cell)
    });
    div()
        .relative()
        .flex_none()
        .w(t.spacing.xxl * 2.)
        .h(t.spacing.xxl * 1.5)
        .children(cells)
}

fn number_badge(t: &Theme, pane: &StripPane) -> impl IntoElement {
    let is_active = pane.tag == Some(Tag::Active);
    div()
        .id(("pane-strip-number", pane.n))
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(t.spacing.icon + t.spacing.xxs)
        .rounded(t.radii.sm)
        .border_1()
        .border_color(if is_active {
            t.colors.accent
        } else {
            t.colors.line
        })
        .font_family(t.typography.numbers_font.clone())
        .text_size(t.typography.tiny)
        .text_color(if is_active {
            t.colors.accent_text
        } else {
            t.colors.muted
        })
        .child(pane.n.to_string())
}

fn tag_label(t: &Theme, pane: &StripPane) -> Option<impl IntoElement> {
    let (id, key, color) = match pane.tag? {
        Tag::Active => (
            "pane-strip-active",
            "pane-strip-active",
            t.colors.accent_text,
        ),
        Tag::Target => ("pane-strip-target", "pane-strip-target", t.colors.muted),
    };
    Some(
        div()
            .id((id, pane.n))
            .test_support()
            .flex_none()
            .text_size(t.typography.tiny)
            .text_color(color)
            .child(rusty_l10n::tr(key)),
    )
}

fn strip_button(t: &Theme, pane: StripPane, cx: &mut Context<FerriteWindow>) -> AnyElement {
    let n = pane.n;
    let label = rusty_l10n::t!("pane-strip-button", n = n, name = pane.name.to_string());
    let is_active = pane.tag == Some(Tag::Active);
    KitButton::new(("pane-strip-button", n))
        .outline()
        .accessibility_label(label)
        .selected(is_active)
        .flex_1()
        .min_w_0()
        .h(t.spacing.tab_height)
        .px(t.spacing.sm)
        .rounded(t.radii.md)
        .bg(t.surfaces.card)
        .border_color(if is_active {
            t.colors.accent
        } else {
            t.surfaces.card_edge
        })
        .text_color(t.colors.text)
        .on_click(cx.listener(move |view, _, window, cx| {
            view.focus_strip_pane(n, window, cx);
        }))
        .child(
            div()
                .flex()
                .items_center()
                .gap(t.spacing.sm)
                .w_full()
                .min_w_0()
                .text_size(t.typography.small)
                .child(number_badge(t, &pane))
                .child(
                    div()
                        .id(("pane-strip-name", n))
                        .test_support()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(pane.name.clone()),
                )
                .children(tag_label(t, &pane)),
        )
        .into_any_element()
}

impl FerriteWindow {
    fn strip_panes(&self, layout: &Layout) -> Vec<StripPane> {
        let state = self.browser.state();
        let focused = state.focused_pane();
        let target = self.browser.target_pane();
        layout
            .numbered()
            .into_iter()
            .enumerate()
            .filter_map(|(index, pane)| {
                let tag = match pane {
                    pane if pane == focused => Some(Tag::Active),
                    pane if Some(pane) == target => Some(Tag::Target),
                    _ => None,
                };
                Some(StripPane {
                    n: index + 1,
                    name: title_of(state.pane(pane)?.active()),
                    tag,
                })
            })
            .collect()
    }

    pub(super) fn pane_strip(&self, layout: &Layout, cx: &mut Context<Self>) -> AnyElement {
        let t = tokens::current(cx).clone();
        let focused = self.browser.state().focused_pane();
        let buttons: Vec<_> = self
            .strip_panes(layout)
            .into_iter()
            .map(|pane| strip_button(&t, pane, cx))
            .collect();
        div()
            .id("pane-strip")
            .role(Role::Group)
            .aria_label(rusty_l10n::tr("pane-strip-title"))
            .test_support()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.sm)
            .pb(t.spacing.xs)
            .child(thumbnail(&t, layout, focused, self.browser.target_pane()))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .gap(t.spacing.xs)
                    .children(buttons),
            )
            .into_any_element()
    }

    fn focus_strip_pane(&mut self, n: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.close_popups(window, cx);
        self.apply(&Command::LayoutFocusPane { n }, window, cx);
    }
}
