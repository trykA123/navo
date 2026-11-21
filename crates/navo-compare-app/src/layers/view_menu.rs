//! The View menu popover (U16b, trimmed by NC128): the Simple/Rich/Custom presets, layout, changed
//! words and wrap. It edits a copy of `UiPrefs`; the host saves it on `Changed` and closes on `Dismissed`.

use gpui_kit::{EventEmitter, FocusHandle, KeyDownEvent};
use navo_compare_profile::appearance::{LayoutPref, UiPrefs, ViewPresetPref};
use sidewise_kit::prelude::*;

/// The two individual View toggles the screen reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toggle {
    Words,
    Wrap,
}

/// What the host should do after a menu change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMenuEvent {
    /// A preference changed; save and apply it.
    Changed,
    /// Close the menu and give focus back (Escape).
    Dismissed,
}

/// One menu entry's meaning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Act {
    Preset(ViewPresetPref),
    Layout(LayoutPref),
    Toggle(Toggle),
}

/// The entries in keyboard order: presets, layouts, toggles.
#[rustfmt::skip]
const ITEMS: [(Act, &str, &str); 7] = [
    (Act::Preset(ViewPresetPref::Simple), "Simple", ""), (Act::Preset(ViewPresetPref::Rich), "Rich", ""),
    (Act::Preset(ViewPresetPref::Custom), "Custom", ""),
    (Act::Layout(LayoutPref::Split), "Side by side", "l"), (Act::Layout(LayoutPref::Unified), "Single file · unified", "l"),
    (Act::Toggle(Toggle::Words), "Changed words inside a line", ""), (Act::Toggle(Toggle::Wrap), "Wrap long lines", "alt-z"),
];

/// The View menu state: the edited preferences and the keyboard-active entry.
pub struct ViewMenu {
    prefs: UiPrefs,
    active: usize,
    handle: FocusHandle,
    /// Whether the last frame ran the opacity entrance; false under reduced motion.
    animated: std::cell::Cell<bool>,
}

impl EventEmitter<ViewMenuEvent> for ViewMenu {}

impl ViewMenu {
    /// A menu over `prefs`; its layout is `prefs.layout`.
    pub fn new(prefs: UiPrefs, cx: &mut Context<Self>) -> Self {
        let active = ITEMS
            .iter()
            .position(|item| item.0 == Act::Preset(prefs.preset))
            .unwrap_or(0);
        Self {
            prefs,
            active,
            handle: cx.focus_handle(),
            animated: std::cell::Cell::new(false),
        }
    }

    /// The preferences as edited so far.
    pub fn prefs(&self) -> &UiPrefs {
        &self.prefs
    }

    /// Moves focus into the menu, so arrows, Enter and Escape reach it.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.handle.focus(window, cx);
    }

    /// Applies one entry: a preset sets its toggles, anything else by hand makes the preset Custom.
    fn apply(&mut self, act: Act, cx: &mut Context<Self>) {
        let p = &mut self.prefs;
        match act {
            Act::Preset(preset) => {
                p.preset = preset;
                if preset != ViewPresetPref::Custom {
                    let rich = preset == ViewPresetPref::Rich;
                    (p.words, p.syntax, p.fold_unchanged) = (rich, rich, rich);
                    p.layout = layout_of(rich);
                }
            }
            Act::Layout(layout) => (p.layout, p.preset) = (layout, ViewPresetPref::Custom),
            Act::Toggle(toggle) => {
                *flag_mut(p, toggle) ^= true;
                p.preset = ViewPresetPref::Custom;
            }
        }
        cx.emit(ViewMenuEvent::Changed);
        cx.notify();
    }

    /// Whether an entry is the chosen preset or layout, or an enabled toggle.
    fn is_on(&self, act: Act) -> bool {
        match act {
            Act::Preset(p) => self.prefs.preset == p,
            Act::Layout(l) => self.prefs.layout == l,
            Act::Toggle(t) => match t {
                Toggle::Words => self.prefs.words,
                Toggle::Wrap => self.prefs.wrap_lines,
            },
        }
    }

    /// Arrow keys move the active entry (wrapping), Enter or Space applies it, Escape dismisses.
    fn key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let stroke = &event.keystroke;
        let n = ITEMS.len();
        match (stroke.modifiers.modified(), stroke.key.as_str()) {
            (false, "down") => self.active = (self.active + 1) % n,
            (false, "up") => self.active = (self.active + n - 1) % n,
            (false, "home") => self.active = 0,
            (false, "end") => self.active = n - 1,
            (false, "enter" | "space") => return self.apply(ITEMS[self.active].0, cx),
            (false, "escape") => cx.emit(ViewMenuEvent::Dismissed),
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }
}

/// The preference field behind one toggle.
fn flag_mut(prefs: &mut UiPrefs, toggle: Toggle) -> &mut bool {
    match toggle {
        Toggle::Words => &mut prefs.words,
        Toggle::Wrap => &mut prefs.wrap_lines,
    }
}

/// Rich shows side by side; Simple shows one unified file.
fn layout_of(rich: bool) -> LayoutPref {
    if rich {
        LayoutPref::Split
    } else {
        LayoutPref::Unified
    }
}

#[path = "view_menu_view.rs"]
mod view;

#[cfg(test)]
#[path = "view_menu_tests.rs"]
mod tests;
