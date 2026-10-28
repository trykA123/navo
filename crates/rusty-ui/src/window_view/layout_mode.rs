use gpui_kit::base::animation::ease_out_cubic;
use gpui_kit::{Animation, AnimationExt, Keystroke, Subscription};
use rusty_core::{Command, Direction, Layout, PaneId};

use super::FerriteWindow;
use crate::prelude::*;
use crate::primitives::Kbd;
use crate::tokens::Theme;

enum ModeKey {
    Core(Command),
    SwitchPane,
}

const HINTS: [(&[&str], &str); 8] = [
    (&["1", "9"], "layout-key-apply"),
    (&["v"], "layout-key-split-right"),
    (&["s"], "layout-key-split-below"),
    (&["x"], "layout-key-close"),
    (&["shift-←", "→"], "layout-key-swap"),
    (&["ctrl-←", "→"], "layout-key-resize"),
    (&["="], "layout-key-even"),
    (&["escape"], "layout-key-done"),
];

fn direction(key: &str) -> Option<Direction> {
    match key {
        "left" => Some(Direction::Left),
        "right" => Some(Direction::Right),
        "up" => Some(Direction::Up),
        "down" => Some(Direction::Down),
        _ => None,
    }
}

fn mode_key(keystroke: &Keystroke) -> Option<ModeKey> {
    let modifiers = &keystroke.modifiers;
    let key = keystroke.key.as_str();
    if modifiers.alt || modifiers.platform || modifiers.function {
        return None;
    }
    let command = match (modifiers.control, modifiers.shift, key) {
        (false, _, "tab") => return Some(ModeKey::SwitchPane),
        (false, _, "=") => Command::LayoutEven,
        (false, false, "v") => Command::LayoutSplit {
            direction: Direction::Right,
        },
        (false, false, "s") => Command::LayoutSplit {
            direction: Direction::Down,
        },
        (false, false, "x") => Command::LayoutClose,
        (false, true, key) => Command::LayoutSwapToward {
            direction: direction(key)?,
        },
        (true, false, key) => Command::LayoutResize {
            direction: direction(key)?,
        },
        (true, true, digit) => Command::LayoutFocusPane {
            n: digit.parse().ok().filter(|n| (1..=4).contains(n))?,
        },
        _ => return None,
    };
    Some(ModeKey::Core(command))
}

fn moved(before: &Layout, after: &Layout) -> Vec<PaneId> {
    let old = before.rects();
    after
        .rects()
        .into_iter()
        .filter(|(pane, rect)| match old.iter().find(|(id, _)| id == pane) {
            None => true,
            Some((_, was)) => {
                was != rect && old.iter().any(|(id, other)| id != pane && other == rect)
            }
        })
        .map(|(pane, _)| pane)
        .collect()
}

#[derive(Default)]
pub(super) struct Fades {
    generation: u64,
    panes: Vec<(PaneId, u64)>,
}

impl Fades {
    fn note(&mut self, before: &Layout, after: &Layout) {
        let fresh = moved(before, after);
        if fresh.is_empty() {
            return;
        }
        self.generation += 1;
        self.panes.retain(|(pane, _)| !fresh.contains(pane));
        let generation = self.generation;
        self.panes
            .extend(fresh.into_iter().map(|pane| (pane, generation)));
    }

    #[cfg(test)]
    pub(super) fn count(&self) -> usize {
        self.panes.len()
    }

    pub(super) fn covers(&self, t: &Theme, layout: &Layout) -> Vec<AnyElement> {
        layout
            .rects()
            .into_iter()
            .filter_map(|(pane, rect)| {
                let (_, generation) = self.panes.iter().find(|(id, _)| *id == pane)?;
                let id = SharedString::from(format!("pane-fade-{}-{generation}", pane.get()));
                let cover = super::tiles::positioned(rect).bg(t.colors.background);
                let fade = Animation::new(t.motion.fade).with_easing(ease_out_cubic);
                Some(
                    cover
                        .with_animation(id, fade, |cover, delta| cover.opacity(1. - delta))
                        .into_any_element(),
                )
            })
            .collect()
    }
}

pub(super) fn key_bar(t: &Theme) -> Div {
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_x(t.spacing.lg)
        .gap_y(t.spacing.xs)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .children(HINTS.iter().enumerate().map(|(index, (keys, label))| {
            div()
                .flex()
                .items_center()
                .gap(t.spacing.xxs)
                .children(keys.iter().enumerate().flat_map(|(at, key)| {
                    let dash = (index == 0 && at == 1).then(|| div().child("–").into_any_element());
                    let cap = Kbd::new(("layout-key", index * 2 + at), key)
                        .map(IntoElement::into_any_element);
                    dash.into_iter().chain(cap)
                }))
                .child(div().ml(t.spacing.xxs).child(rusty_l10n::tr(label)))
        }))
}

pub(super) fn intercept_mode_keys(cx: &mut Context<FerriteWindow>) -> Subscription {
    let this = cx.weak_entity();
    cx.intercept_keystrokes(move |event, window, cx| {
        let Some(key) = mode_key(&event.keystroke) else {
            return;
        };
        let _ = this.update(cx, |view, cx| {
            let is_open = view
                .tiles
                .switcher
                .as_ref()
                .is_some_and(|state| state.focus.is_focused(window));
            if is_open {
                cx.stop_propagation();
                view.layout_mode_key(key, window, cx);
            }
        });
    })
}

impl FerriteWindow {
    fn layout_mode_key(&mut self, key: ModeKey, window: &mut Window, cx: &mut Context<Self>) {
        let before = self.tiles_layout();
        match key {
            ModeKey::Core(command) => self.apply(&command, window, cx),
            ModeKey::SwitchPane => self.switch_pane(window, cx),
        }
        if !cx.reduce_motion() {
            self.tiles.fades.note(&before, &self.tiles_layout());
        }
        if let Some(state) = &self.tiles.switcher {
            window.focus(&state.focus, cx);
        }
        cx.notify();
    }
}
