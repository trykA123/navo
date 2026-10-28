use gpui_kit::base::TestSupportExt;
use gpui_kit::base::animation::ease_out_cubic;
use gpui_kit::component::Selectable;
use gpui_kit::component::button::Button as KitButton;
use gpui_kit::{Animation, AnimationExt, Role};
use rusty_core::{Command, Snapshot};

use super::FerriteWindow;
use super::workspace_keys::Preview;
use crate::prelude::*;
use crate::primitives::{IconButton, IconName, Kbd, icon};
use crate::tokens::{self, Theme};

struct Card<'a> {
    n: usize,
    name: &'a str,
    snapshot: &'a Snapshot,
    active: bool,
    preview: bool,
}

fn miniature(t: &Theme, snapshot: &Snapshot) -> Div {
    let panes = snapshot.panes();
    div().relative().w_full().h(t.spacing.xxl * 5.).children(
        snapshot
            .layout()
            .rects()
            .into_iter()
            .filter_map(|(id, rect)| {
                let pane = panes.iter().find(|pane| pane.id() == id)?;
                let folder = pane.active().location();
                let label = folder.name().map_or_else(
                    || folder.path().display().into_owned(),
                    |name| name.display().into_owned(),
                );
                Some(
                    div()
                        .absolute()
                        .left(relative(rect.x as f32))
                        .top(relative(rect.y as f32))
                        .w(relative(rect.width as f32))
                        .h(relative(rect.height as f32))
                        .p(t.spacing.xxs)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .size_full()
                                .px(t.spacing.xs)
                                .rounded(t.radii.sm)
                                .border_1()
                                .border_color(t.surfaces.card_edge)
                                .bg(t.surfaces.field)
                                .text_size(t.typography.small)
                                .text_color(t.colors.muted)
                                .child(div().min_w_0().truncate().child(label)),
                        ),
                )
            }),
    )
}

fn metadata(t: &Theme, card: &Card) -> Div {
    let open = rusty_l10n::tr("workspace-open");
    div()
        .flex()
        .items_center()
        .gap(t.spacing.sm)
        .w_full()
        .children(Kbd::new(("workspace-key", card.n), &card.n.to_string()))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .child(card.name.to_owned()),
        )
        .when(card.active, |row| {
            row.child(
                div()
                    .id("workspace-open")
                    .role(Role::Label)
                    .aria_label(open.clone())
                    .test_support()
                    .text_size(t.typography.small)
                    .text_color(t.colors.accent_text)
                    .child(open),
            )
        })
}

fn card_button(t: &Theme, id: impl Into<ElementId>, label: String) -> KitButton {
    KitButton::new(id)
        .outline()
        .accessibility_label(label)
        .flex_none()
        .w(t.spacing.xxl * 14.)
        .h(t.spacing.xxl * 8.)
        .p(t.spacing.md)
        .rounded(t.radii.card)
        .bg(t.surfaces.card)
        .border_color(t.surfaces.card_edge)
        .text_color(t.colors.text)
}

fn workspace_card(t: &Theme, card: Card, cx: &mut Context<FerriteWindow>) -> AnyElement {
    let n = card.n;
    let button = card_button(t, ("workspace-switch", n), card.name.to_owned())
        .selected(card.preview)
        .when(card.preview, |button| button.border_color(t.colors.accent))
        .on_click(cx.listener(move |view, _, window, cx| {
            view.choose_workspace(Command::WorkspaceSwitch { n }, window, cx);
        }))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(t.spacing.md)
                .size_full()
                .child(miniature(t, card.snapshot))
                .child(metadata(t, &card)),
        );
    div()
        .id(("workspace-card", n))
        .role(Role::Group)
        .aria_selected(card.preview)
        .test_support()
        .flex()
        .flex_none()
        .child(button)
        .into_any_element()
}

fn new_card(t: &Theme, preview: bool, cx: &mut Context<FerriteWindow>) -> AnyElement {
    let label = rusty_l10n::tr("workspace-new-from-layout");
    let button = card_button(t, "workspace-new-switch", label.clone())
        .selected(preview)
        .when(preview, |button| button.border_color(t.colors.accent))
        .on_click(cx.listener(|view, _, window, cx| {
            view.choose_workspace(Command::WorkspaceNew { name: None }, window, cx);
        }))
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(t.spacing.md)
                .size_full()
                .child(icon(IconName::Plus, t.spacing.icon, t.colors.muted))
                .child(label)
                .children(Kbd::new("workspace-new-key", "alt-n")),
        );
    div()
        .id("workspace-new")
        .role(Role::Group)
        .aria_selected(preview)
        .test_support()
        .flex()
        .flex_none()
        .child(button)
        .into_any_element()
}

fn header(t: &Theme, cx: &mut Context<FerriteWindow>) -> Div {
    div()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .child(rusty_l10n::tr("workspace-tray-title")),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(rusty_l10n::tr("workspace-tray-hint")),
        )
        .child(
            IconButton::new(
                "workspace-close",
                IconName::X,
                rusty_l10n::tr("workspace-tray-close"),
            )
            .on_click(cx.listener(|view, _, _, cx| {
                view.workspace_keys.cancel();
                cx.notify();
            })),
        )
}

fn tray_surface(t: &Theme, bottom: Pixels) -> gpui_kit::Stateful<Div> {
    div()
        .id("workspace-tray")
        .role(Role::Group)
        .aria_label(rusty_l10n::tr("workspace-tray-title"))
        .occlude()
        .absolute()
        .left_0()
        .right_0()
        .bottom(bottom)
        .flex()
        .flex_col()
        .gap(t.spacing.md)
        .p(t.spacing.lg)
        .bg(t.surfaces.sheet)
        .border_t_1()
        .border_color(t.surfaces.card_edge)
        .shadow(t.shadows.up.clone())
}

impl FerriteWindow {
    fn choose_workspace(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        self.workspace_keys.cancel();
        self.apply(&command, window, cx);
    }

    fn workspace_cards(&self, t: &Theme, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let state = self.browser.state();
        let Some(core) = state.window(state.focused_window()) else {
            return Vec::new();
        };
        let current = core.snapshot();
        core.workspaces()
            .iter()
            .enumerate()
            .map(|(index, workspace)| {
                let active = index == core.active_workspace();
                let snapshot = if active {
                    &current
                } else {
                    workspace.live.as_ref().unwrap_or(&workspace.saved)
                };
                workspace_card(
                    t,
                    Card {
                        n: index + 1,
                        name: &workspace.name,
                        snapshot,
                        active,
                        preview: self.workspace_keys.preview == Some(Preview::Existing(index + 1)),
                    },
                    cx,
                )
            })
            .collect()
    }

    pub(super) fn workspace_tray(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.workspace_keys.visible {
            return None;
        }
        let t = tokens::current(cx).clone();
        let strip = self.fkey_strip_height(&t);
        let inset = if strip > px(0.) { px(0.) } else { t.spacing.md };
        let bottom = self
            .hint_cover(window, cx)
            .map_or(t.spacing.status_height + t.spacing.xs + inset, |cover| {
                window.viewport_size().height - strip - cover.top
            });
        let cards = self.workspace_cards(&t, cx);
        let tray = tray_surface(&t, bottom)
            .test_support()
            .child(header(&t, cx))
            .child(
                div()
                    .id("workspace-cards")
                    .test_support()
                    .flex()
                    .min_w_0()
                    .w_full()
                    .gap(t.spacing.md)
                    .overflow_x_scroll()
                    .children(cards)
                    .child(new_card(
                        &t,
                        self.workspace_keys.preview == Some(Preview::New),
                        cx,
                    )),
            );
        Some(if cx.reduce_motion() {
            tray.into_any_element()
        } else {
            tray.with_animation(
                "workspace-tray-enter",
                Animation::new(t.motion.fade).with_easing(ease_out_cubic),
                |tray, delta| tray.opacity(delta),
            )
            .into_any_element()
        })
    }

    pub(super) fn workspace_ribbon(&self, cx: &App) -> Option<AnyElement> {
        let preview = self.workspace_keys.preview?;
        let state = self.browser.state();
        let core = state.window(state.focused_window())?;
        let name = match preview {
            Preview::Existing(n) => core.workspaces().get(n.checked_sub(1)?)?.name.clone(),
            Preview::New => rusty_l10n::tr("workspace-new-from-layout"),
        };
        let label = rusty_l10n::t!("workspace-preview-ribbon", name = name);
        let t = tokens::current(cx);
        Some(
            div()
                .id("workspace-preview-ribbon")
                .role(Role::Status)
                .aria_label(label.clone())
                .test_support()
                .min_w_0()
                .truncate()
                .text_size(t.typography.small)
                .text_color(t.colors.accent_text)
                .child(label)
                .into_any_element(),
        )
    }
}
