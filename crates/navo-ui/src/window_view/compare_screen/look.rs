//! The look of the file diff (D58): Simple, Rich or Custom, which parts show, and the layout.
//! The View menu changes it; the file diff and its host read it.

use super::file::layout_for;
use crate::compare_diff::Layout;
use crate::prelude::*;

/// The three presets of the View menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::window_view) enum Preset {
    /// A line diff: no changed words, no change map.
    Simple,
    /// Every part on.
    Rich,
    /// The parts the user set one by one.
    Custom,
}

/// What the file diff shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::window_view) struct Look {
    pub(in crate::window_view) preset: Preset,
    /// Changed words inside a line.
    pub(in crate::window_view) words: bool,
    /// The change map on the right edge.
    pub(in crate::window_view) map: bool,
    /// The layout the user picked; none follows the window width.
    pub(in crate::window_view) layout: Option<Layout>,
    /// Wrap long lines; off by default. Kept by every preset (D39.8).
    pub(in crate::window_view) wrap: bool,
}

impl Look {
    /// The Rich look with the layout following the window.
    pub(in crate::window_view) fn rich() -> Self {
        Self {
            preset: Preset::Rich,
            words: true,
            map: true,
            layout: None,
            wrap: false,
        }
    }

    /// Picks a preset. Simple and Rich set the parts; Custom keeps them as they are.
    pub(in crate::window_view) fn choose(&mut self, preset: Preset) {
        let (words, map) = match preset {
            Preset::Simple => (false, false),
            Preset::Rich => (true, true),
            Preset::Custom => (self.words, self.map),
        };
        *self = Self {
            preset,
            words,
            map,
            ..*self
        };
    }

    /// Flips the changed words; the look becomes Custom.
    pub(in crate::window_view) fn toggle_words(&mut self) {
        self.words = !self.words;
        self.preset = Preset::Custom;
    }

    /// Flips the change map; the look becomes Custom.
    pub(in crate::window_view) fn toggle_map(&mut self) {
        self.map = !self.map;
        self.preset = Preset::Custom;
    }

    /// Flips line wrap; every preset keeps wrap (D39.8), so the preset stays as it is.
    pub(in crate::window_view) fn toggle_wrap(&mut self) {
        self.wrap = !self.wrap;
    }

    /// The layout in `window`: the picked one, else side by side, or unified below 900 px.
    pub(in crate::window_view) fn layout_in(&self, window: &Window) -> Layout {
        self.layout.unwrap_or_else(|| layout_for(window))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_starts_off_and_every_preset_keeps_it() {
        let mut look = Look::rich();
        assert!(!look.wrap);
        look.wrap = true;
        for preset in [Preset::Simple, Preset::Rich, Preset::Custom] {
            look.choose(preset);
            assert!(look.wrap, "{preset:?} dropped wrap");
        }
    }
}
