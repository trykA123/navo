use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::base::animation::ease_out_cubic;
use gpui_kit::component::Selectable;
use gpui_kit::component::button::{Button as KitButton, ButtonVariants};
use gpui_kit::{
    Animation, AnimationExt, FocusHandle, KeyDownEvent, RenderOnce, Role, Subscription,
};
use rusty_core::{Command, Layout, PaneId, Tiles};

use super::FerriteWindow;
use crate::prelude::*;
use crate::primitives::Kbd;
use crate::tokens::{self, Theme};

gpui_kit::actions!(ferrite_window, [ShowLayoutSwitcher]);

pub fn builtin_layout_presets() -> Vec<(SharedString, Tiles)> {
    let names = [
        "layout-preset-1",
        "layout-preset-2",
        "layout-preset-3",
        "layout-preset-4",
        "layout-preset-5",
    ];
    Tiles::builtin()
        .into_iter()
        .zip(names)
        .map(|(preset, key)| (rusty_l10n::tr(key).into(), preset.tiles))
        .collect()
}

pub fn layout_presets(user: &[(SharedString, Tiles)]) -> Vec<(SharedString, Tiles)> {
    let mut presets = builtin_layout_presets();
    presets.extend_from_slice(user);
    presets
}

#[derive(Clone, Copy)]
pub enum LayoutSwitcherAction {
    Apply(usize),
    Move(i8),
    Close,
}

type OnAction = Rc<dyn Fn(&LayoutSwitcherAction, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct LayoutSwitcher {
    presets: Vec<(SharedString, Tiles)>,
    current: Tiles,
    focused: usize,
    slots: (usize, Option<usize>),
    on_action: OnAction,
    scroll: ScrollHandle,
}

impl LayoutSwitcher {
    pub fn new(
        presets: Vec<(SharedString, Tiles)>,
        current: Tiles,
        on_action: impl Fn(&LayoutSwitcherAction, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            presets,
            current,
            focused: 0,
            slots: (0, Some(1)),
            on_action: Rc::new(on_action),
            scroll: ScrollHandle::new(),
        }
    }

    #[must_use]
    pub fn focused(mut self, focused: usize) -> Self {
        self.focused = focused;
        self
    }

    #[must_use]
    pub fn scroll(mut self, scroll: ScrollHandle) -> Self {
        self.scroll = scroll;
        self
    }

    #[must_use]
    pub fn slots(mut self, focused: usize, target: Option<usize>) -> Self {
        self.slots = (focused, target);
        self
    }
}

fn thumbnail(t: &Theme, tiles: &Tiles, slots: (usize, Option<usize>)) -> Div {
    let mut next = 0;
    let mut layout = Layout::new(PaneId::new(0));
    layout.apply(tiles, &mut || {
        next += 1;
        PaneId::new(next)
    });
    let numbered = layout.numbered();
    let focused = numbered.get(slots.0).or_else(|| numbered.first());
    let target = slots
        .1
        .and_then(|slot| numbered.get(slot))
        .filter(|pane| Some(*pane) != focused)
        .or_else(|| numbered.iter().find(|pane| Some(*pane) != focused));
    div()
        .relative()
        .w(t.spacing.xxl * 4.)
        .h(t.spacing.xxl * 3.)
        .children(layout.rects().into_iter().map(|(pane, rect)| {
            let active = focused == Some(&pane);
            let target = target == Some(&pane);
            div()
                .absolute()
                .left(relative(rect.x as f32))
                .top(relative(rect.y as f32))
                .w(relative(rect.width as f32))
                .h(relative(rect.height as f32))
                .p(t.spacing.xxs)
                .child(
                    div()
                        .size_full()
                        .rounded(t.radii.sm)
                        .border_1()
                        .border_color(if active || target {
                            t.colors.accent
                        } else {
                            t.colors.line
                        })
                        .when(active, |cell| cell.bg(t.colors.accent)),
                )
        }))
}

impl LayoutSwitcher {
    fn card(&self, t: &Theme, index: usize) -> AnyElement {
        let (name, tiles) = &self.presets[index];
        let on_action = self.on_action.clone();
        KitButton::new(("layout-preset", index + 1))
            .ghost()
            .accessibility_label(name.clone())
            .selected(*tiles == self.current)
            .toggled(*tiles == self.current)
            .tab_stop(false)
            .flex_none()
            .h_auto()
            .p(t.spacing.md)
            .rounded(t.radii.md)
            .border_1()
            .border_color(if index == self.focused {
                t.colors.accent
            } else {
                t.surfaces.card_edge
            })
            .bg(if *tiles == self.current {
                t.surfaces.current
            } else {
                t.surfaces.card
            })
            .text_color(if *tiles == self.current {
                t.colors.on_selection
            } else {
                t.colors.text
            })
            .on_click(move |_, window, cx| {
                on_action(&LayoutSwitcherAction::Apply(index), window, cx)
            })
            .child(self.card_content(t, index))
            .into_any_element()
    }

    fn card_content(&self, t: &Theme, index: usize) -> Div {
        let (name, tiles) = &self.presets[index];
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(t.spacing.md)
            .child(thumbnail(t, tiles, self.slots))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(t.spacing.sm)
                    .children(if index < 9 {
                        Kbd::new(("layout-digit", index + 1), &(index + 1).to_string())
                    } else {
                        None
                    })
                    .child(name.clone()),
            )
    }
}

impl RenderOnce for LayoutSwitcher {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx);
        div()
            .id("layout-switcher")
            .role(Role::Group)
            .aria_label(rusty_l10n::tr("layout-switcher-title"))
            .test_support()
            .occlude()
            .flex()
            .flex_col()
            .gap(t.spacing.md)
            .max_w_full()
            .max_h_full()
            .p(t.spacing.xl)
            .rounded(t.radii.card)
            .border_1()
            .border_color(t.surfaces.card_edge)
            .bg(t.surfaces.sheet)
            .shadow(t.shadows.up.clone())
            .child(rusty_l10n::tr("layout-switcher-title"))
            .child(
                div()
                    .id("layout-presets")
                    .flex()
                    .flex_wrap()
                    .min_h_0()
                    .gap(t.spacing.md)
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .children((0..self.presets.len()).map(|index| self.card(t, index))),
            )
            .child(super::layout_mode::key_bar(
                t,
                self.focused >= Tiles::builtin().len(),
            ))
    }
}

pub(super) struct SwitcherState {
    focused: usize,
    pub(super) focus: FocusHandle,
    pub(super) edited: bool,
    scroll: ScrollHandle,
    _keys: Subscription,
}

impl FerriteWindow {
    fn switcher_presets(&self) -> Vec<(SharedString, Tiles)> {
        layout_presets(self.values.layout_presets())
    }

    pub(super) fn switcher_focus(&self, state: &SwitcherState) -> usize {
        let count = Tiles::builtin().len() + self.values.layout_presets().len();
        state.focused.min(count.saturating_sub(1))
    }

    fn open_layout_switcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let presets = self.switcher_presets();
        let current = self.tiles_layout().to_tiles();
        let focused = presets
            .iter()
            .position(|(_, tiles)| *tiles == current)
            .unwrap_or(0);
        let focus = cx.focus_handle();
        self.workspace_keys.cancel();
        window.focus(&focus, cx);
        let scroll = ScrollHandle::new();
        scroll.scroll_to_item(focused);
        self.tiles.switcher = Some(SwitcherState {
            focused,
            focus,
            edited: false,
            scroll,
            _keys: super::layout_mode::intercept_mode_keys(cx),
        });
        cx.notify();
    }

    fn switch_layout(
        &mut self,
        action: LayoutSwitcherAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut presets = self.switcher_presets();
        let Some(state) = self.tiles.switcher.as_mut() else {
            return;
        };
        match action {
            LayoutSwitcherAction::Apply(index) => {
                if index >= presets.len() {
                    return;
                }
                let (_, tiles) = presets.swap_remove(index);
                self.tiles.switcher = None;
                self.apply(&Command::LayoutPreset { tiles }, window, cx);
            }
            LayoutSwitcherAction::Move(by) => {
                state.edited = false;
                state.focused = state
                    .focused
                    .saturating_add_signed(isize::from(by))
                    .min(presets.len().saturating_sub(1));
                state.scroll.scroll_to_item(state.focused);
                cx.notify();
            }
            LayoutSwitcherAction::Close => {
                self.tiles.switcher = None;
                self.focus_core_pane(window, cx);
                cx.notify();
            }
        }
    }

    fn layout_switcher_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.tiles.switcher.as_ref() else {
            return;
        };
        cx.stop_propagation();
        let key = &event.keystroke;
        let action = match key.key.as_str() {
            "escape" => LayoutSwitcherAction::Close,
            _ if key.modifiers.modified() => return,
            "left" | "up" => LayoutSwitcherAction::Move(-1),
            "right" | "down" => LayoutSwitcherAction::Move(1),
            "enter" if state.edited => LayoutSwitcherAction::Close,
            "enter" => LayoutSwitcherAction::Apply(self.switcher_focus(state)),
            digit => {
                let Some(index) = digit.parse::<usize>().ok().and_then(|n| n.checked_sub(1)) else {
                    return;
                };
                LayoutSwitcherAction::Apply(index)
            }
        };
        self.switch_layout(action, window, cx);
    }

    pub(super) fn layout_switcher_on(root: Div, cx: &mut Context<Self>) -> Div {
        root.on_action(cx.listener(|view, _: &ShowLayoutSwitcher, window, cx| {
            view.open_layout_switcher(window, cx);
        }))
    }

    pub(super) fn layout_switcher_view(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let state = self.tiles.switcher.as_ref()?;
        let layout = self.tiles_layout();
        let numbered = layout.numbered();
        let core = self
            .browser
            .state()
            .window(self.browser.state().focused_window())?;
        let focused = numbered
            .iter()
            .position(|pane| *pane == layout.focused())
            .unwrap_or(0);
        let target = core
            .target()
            .and_then(|id| numbered.iter().position(|pane| *pane == id));
        let switcher = LayoutSwitcher::new(
            self.switcher_presets(),
            layout.to_tiles(),
            cx.listener(|view, action: &LayoutSwitcherAction, window, cx| {
                view.switch_layout(*action, window, cx)
            }),
        )
        .focused(self.switcher_focus(state))
        .slots(focused, target)
        .scroll(state.scroll.clone());
        let t = tokens::current(cx);
        let layer = div()
            .id("layout-switcher-layer")
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .p(t.spacing.md)
            .track_focus(&state.focus)
            .key_context("LayoutSwitcher")
            .capture_key_down(cx.listener(Self::layout_switcher_key))
            .child(switcher);
        Some(if cx.reduce_motion() {
            layer.into_any_element()
        } else {
            layer
                .with_animation(
                    "layout-switcher-enter",
                    Animation::new(t.motion.fade).with_easing(ease_out_cubic),
                    |layer, delta| layer.opacity(delta),
                )
                .into_any_element()
        })
    }
}
