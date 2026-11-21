//! The Theme picker's state (NC150, NC151): five palettes and three modes in one list. The host
//! saves the choice on `Changed` and closes the picker on `Dismissed`.

use std::cell::Cell;

use gpui_kit::{EventEmitter, FocusHandle, KeyDownEvent};
use navo_compare_profile::appearance::{AppearanceMode, PaletteId};
use sidewise_kit::prelude::*;

/// One picker row's meaning: a palette or a mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    Palette(PaletteId),
    Mode(AppearanceMode),
}

/// What the host should do after a picker change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeMenuEvent {
    /// The palette or mode changed; save and apply it.
    Changed,
    /// Close the picker and give focus back (Escape).
    Dismissed,
}

/// Keyboard order: the five palettes in `PaletteId` order, then the three modes.
#[rustfmt::skip]
pub(crate) const ITEMS: [(Pick, &str); 8] = [
    (Pick::Palette(PaletteId::Violet), "Violet"), (Pick::Palette(PaletteId::Graphite), "Graphite"),
    (Pick::Palette(PaletteId::Ocean), "Ocean"), (Pick::Palette(PaletteId::Ember), "Ember"),
    (Pick::Palette(PaletteId::Contrast), "High contrast"),
    (Pick::Mode(AppearanceMode::System), "System"), (Pick::Mode(AppearanceMode::Light), "Light"),
    (Pick::Mode(AppearanceMode::Dark), "Dark"),
];

/// The picker state: the chosen palette and mode, and the keyboard-active row.
pub struct ThemeMenu {
    palette: PaletteId,
    mode: AppearanceMode,
    active: usize,
    handle: FocusHandle,
    /// Whether the last frame ran the opacity entrance; false under reduced motion.
    animated: Cell<bool>,
}

impl EventEmitter<ThemeMenuEvent> for ThemeMenu {}

impl ThemeMenu {
    /// A picker over the current `palette` and `mode`; the active row is the palette's row.
    pub fn new(palette: PaletteId, mode: AppearanceMode, cx: &mut Context<Self>) -> Self {
        let active = ITEMS
            .iter()
            .position(|item| item.0 == Pick::Palette(palette))
            .unwrap_or(0);
        Self {
            palette,
            mode,
            active,
            handle: cx.focus_handle(),
            animated: Cell::new(false),
        }
    }

    /// The palette and mode as chosen so far.
    pub fn choice(&self) -> (PaletteId, AppearanceMode) {
        (self.palette, self.mode)
    }

    /// Moves focus into the picker, so arrows, Enter and Escape reach it.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.handle.focus(window, cx);
    }

    /// Whether `pick` is the current palette or mode.
    fn is_on(&self, pick: Pick) -> bool {
        match pick {
            Pick::Palette(p) => p == self.palette,
            Pick::Mode(m) => m == self.mode,
        }
    }

    /// Sets the pick; emits `Changed` and notifies only when the value changes.
    fn apply(&mut self, pick: Pick, cx: &mut Context<Self>) {
        if self.is_on(pick) {
            return;
        }
        match pick {
            Pick::Palette(p) => self.palette = p,
            Pick::Mode(m) => self.mode = m,
        }
        cx.emit(ThemeMenuEvent::Changed);
        cx.notify();
    }
}

impl ThemeMenu {
    /// Arrows move the active row (wrapping); Enter or Space picks it unless held; Escape dismisses.
    fn key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let stroke = &event.keystroke;
        let n = ITEMS.len();
        match (stroke.modifiers.modified(), stroke.key.as_str()) {
            (false, "down") => self.active = (self.active + 1) % n,
            (false, "up") => self.active = (self.active + n - 1) % n,
            (false, "home") => self.active = 0,
            (false, "end") => self.active = n - 1,
            (false, "enter" | "space") => {
                cx.stop_propagation();
                if !event.is_held {
                    self.apply(ITEMS[self.active].0, cx);
                }
                return;
            }
            (false, "escape") => cx.emit(ThemeMenuEvent::Dismissed),
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }
}

#[path = "theme_menu_view.rs"]
mod view;

#[cfg(test)]
#[path = "theme_menu_tests.rs"]
mod tests;
