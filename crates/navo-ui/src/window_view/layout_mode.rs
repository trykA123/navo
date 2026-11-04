use gpui_kit::base::animation::ease_out_cubic;
use gpui_kit::{Animation, AnimationExt, Keystroke, Subscription};
use navo_core::{Command, Direction, Layout, PaneId};

use super::NavoWindow;
use super::layout_switcher::LayoutSwitcherAction;
use super::pane::PaneStep;
use crate::commands::{Keymap, LAYOUT_MODE, active, is_fixed_digit, is_shown_key};
use crate::prelude::*;
use crate::primitives::Kbd;
use crate::tokens::Theme;

gpui_kit::actions!(
    navo_window,
    [
        ClosePane,
        FocusPane1,
        FocusPane2,
        FocusPane3,
        FocusPane4,
        PreviousPane,
        SaveLayout,
        SplitPaneBelow,
        SplitPaneRight
    ]
);

enum ModeKey {
    Core(Command),
    SwitchPane,
    Save,
    Remove,
    Done,
}

struct Hint {
    label: &'static str,
    commands: &'static [&'static str],
    range: bool,
}

const fn hint(label: &'static str, commands: &'static [&'static str]) -> Hint {
    Hint {
        label,
        commands,
        range: false,
    }
}

const REMOVE: &str = "layout_mode.remove";
const APPLY_LABEL: &str = "layout-key-apply";

const HINTS: [Hint; 11] = [
    hint("layout-key-split-right", &["layout_mode.split_right"]),
    hint("layout-key-split-below", &["layout_mode.split_below"]),
    hint("layout-key-close", &["layout_mode.close"]),
    hint(
        "layout-key-swap",
        &["layout_mode.swap_left", "layout_mode.swap_right"],
    ),
    hint(
        "layout-key-resize",
        &["layout_mode.resize_left", "layout_mode.resize_right"],
    ),
    hint("layout-key-even", &["layout_mode.even"]),
    hint("layout-key-save", &["layout_mode.save"]),
    hint("layout-key-remove", &[REMOVE]),
    hint("layout-key-target", &["layout_mode.target"]),
    Hint {
        label: "layout-key-pane",
        commands: &[
            "layout_mode.focus_pane_first",
            "layout_mode.focus_pane_fourth",
        ],
        range: true,
    },
    hint("layout-key-done", &["layout_mode.done"]),
];

fn core(command: Command) -> ModeKey {
    ModeKey::Core(command)
}

fn split(direction: Direction) -> ModeKey {
    core(Command::LayoutSplit { direction })
}

fn swap(direction: Direction) -> ModeKey {
    core(Command::LayoutSwapToward { direction })
}

fn resize(direction: Direction) -> ModeKey {
    core(Command::LayoutResize { direction })
}

fn pane(n: usize) -> ModeKey {
    core(Command::LayoutFocusPane { n })
}

type Verb = (&'static str, fn() -> ModeKey);

/// Each verb of the layout mode with what it does. The digits 1-9 stay fixed.
const VERBS: [Verb; 20] = [
    ("layout_mode.split_right", || split(Direction::Right)),
    ("layout_mode.split_below", || split(Direction::Down)),
    ("layout_mode.close", || core(Command::LayoutClose)),
    ("layout_mode.swap_left", || swap(Direction::Left)),
    ("layout_mode.swap_right", || swap(Direction::Right)),
    ("layout_mode.swap_up", || swap(Direction::Up)),
    ("layout_mode.swap_down", || swap(Direction::Down)),
    ("layout_mode.resize_left", || resize(Direction::Left)),
    ("layout_mode.resize_right", || resize(Direction::Right)),
    ("layout_mode.resize_up", || resize(Direction::Up)),
    ("layout_mode.resize_down", || resize(Direction::Down)),
    ("layout_mode.even", || core(Command::LayoutEven)),
    ("layout_mode.save", || ModeKey::Save),
    (REMOVE, || ModeKey::Remove),
    ("layout_mode.target", || ModeKey::SwitchPane),
    ("layout_mode.focus_pane_first", || pane(1)),
    ("layout_mode.focus_pane_second", || pane(2)),
    ("layout_mode.focus_pane_third", || pane(3)),
    ("layout_mode.focus_pane_fourth", || pane(4)),
    ("layout_mode.done", || ModeKey::Done),
];

fn mode_key(keymap: &Keymap, keystroke: &Keystroke) -> Option<ModeKey> {
    if is_fixed_digit(&keystroke.unparse()) {
        return None;
    }
    VERBS
        .iter()
        .find(|(name, _)| keymap.has_key_in(name, LAYOUT_MODE, keystroke))
        .map(|(_, key)| key())
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

pub(super) fn key_bar(t: &Theme, can_remove: bool, keymap: &Keymap) -> Div {
    let apply = Hint {
        label: APPLY_LABEL,
        commands: &[],
        range: true,
    };
    let fixed = vec!["1".to_owned(), "9".to_owned()];
    let verbs = HINTS
        .iter()
        .enumerate()
        .filter(|(_, hint)| can_remove || hint.commands != [REMOVE])
        .map(|(at, hint)| (at + 1, hint, hint_keys(keymap, hint)))
        .filter(|(_, _, keys)| !keys.is_empty());
    let rows = std::iter::once((0, &apply, fixed)).chain(verbs);
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_x(t.spacing.lg)
        .gap_y(t.spacing.xs)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .children(rows.map(|(index, hint, keys)| hint_view(t, index, hint, &keys)))
}

#[cfg(test)]
pub(super) fn split_right_hint(cx: &App) -> Option<String> {
    hint_keys(active(cx), &HINTS[0]).into_iter().next()
}

/// The keys of a verb, one per shown command (a pair for the ranges); later ones lose the modifiers they share with the first.
fn hint_keys(keymap: &Keymap, hint: &Hint) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    for command in hint.commands {
        let Some(key) = keymap
            .keys_in(command, LAYOUT_MODE)
            .find(|key| is_shown_key(command, key))
        else {
            continue;
        };
        let shared = keys
            .first()
            .and_then(|first| first.rsplit_once('-'))
            .zip(key.rsplit_once('-'))
            .filter(|(first, this)| first.0 == this.0)
            .map(|(_, this)| this.1);
        keys.push(shared.unwrap_or(key).to_owned());
    }
    keys
}

fn hint_view(t: &Theme, index: usize, hint: &Hint, keys: &[String]) -> Div {
    div()
        .flex()
        .items_center()
        .gap(t.spacing.xxs)
        .children(keys.iter().enumerate().flat_map(|(at, key)| {
            let dash = (hint.range && at == 1).then(|| div().child("–").into_any_element());
            let cap =
                Kbd::new(("layout-key", index * 2 + at), key).map(IntoElement::into_any_element);
            dash.into_iter().chain(cap)
        }))
        .child(div().ml(t.spacing.xxs).child(navo_l10n::tr(hint.label)))
}

pub(super) fn intercept_mode_keys(cx: &mut Context<NavoWindow>) -> Subscription {
    let this = cx.weak_entity();
    cx.intercept_keystrokes(move |event, window, cx| {
        let Some(key) = mode_key(active(cx), &event.keystroke) else {
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

impl NavoWindow {
    fn layout_mode_key(&mut self, key: ModeKey, window: &mut Window, cx: &mut Context<Self>) {
        let before = self.tiles_layout();
        match key {
            ModeKey::Core(command) => self.apply(&command, window, cx),
            ModeKey::SwitchPane => self.step_pane(PaneStep::Target, window, cx),
            ModeKey::Done => return self.switch_layout(LayoutSwitcherAction::Close, window, cx),
            ModeKey::Save => return self.ask_save_layout(cx),
            ModeKey::Remove => {
                if let Some(state) = &self.tiles.switcher {
                    self.ask_remove_layout(self.switcher_focus(state), cx);
                }
                return;
            }
        }
        let after = self.tiles_layout();
        if !cx.reduce_motion() && !self.is_one_pane(window) {
            self.tiles.fades.note(&before, &after);
        }
        if let Some(state) = &mut self.tiles.switcher {
            state.edited |= before.rects() != after.rects();
            window.focus(&state.focus, cx);
        }
        cx.notify();
    }
}

macro_rules! pane_number_keys {
    ($root:expr, $cx:expr, $($action:ident => $n:literal),+ $(,)?) => {
        $root$(.on_action($cx.listener(|view: &mut NavoWindow, _: &$action, window, cx| {
            view.apply(&Command::LayoutFocusPane { n: $n }, window, cx);
        })))+
    };
}

macro_rules! layout_command_keys {
    ($root:expr, $cx:expr, $($action:ident => $command:expr),+ $(,)?) => {
        $root$(.on_action($cx.listener(|view: &mut NavoWindow, _: &$action, window, cx| {
            view.layout_mode_key(ModeKey::Core($command), window, cx);
        })))+
    };
}

pub(super) fn layout_keys(root: Div, cx: &mut Context<NavoWindow>) -> Div {
    let root = pane_number_keys!(root, cx,
        FocusPane1 => 1,
        FocusPane2 => 2,
        FocusPane3 => 3,
        FocusPane4 => 4,
    );
    let root = layout_command_keys!(root, cx,
        SplitPaneRight => Command::LayoutSplit { direction: Direction::Right },
        SplitPaneBelow => Command::LayoutSplit { direction: Direction::Down },
        ClosePane => Command::LayoutClose,
    );
    root.on_action(cx.listener(|view, _: &PreviousPane, window, cx| {
        view.step_pane(PaneStep::Previous, window, cx);
    }))
    .on_action(cx.listener(|view, _: &SaveLayout, _, cx| {
        view.ask_save_layout(cx);
    }))
}
