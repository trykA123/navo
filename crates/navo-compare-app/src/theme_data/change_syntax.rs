//! The semantic change language and the syntax roles of Sidewise (NC66): eight change meanings
//! that keep their hue across palettes, and seven syntax roles that follow the palette.

use std::sync::LazyLock;

use gpui_kit::Hsla;
use navo_compare_profile::appearance::PaletteId;
use sidewise_kit::tokens::Mode;

use super::palettes::{Oklch, oklch};

/// The eight change meanings, in the order of the tone arrays below.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    Add,
    Del,
    Chg,
    Mov,
    Ol,
    Or,
    Cfl,
    Same,
}

/// One change meaning: its text, line wash, stronger word fill and gutter/ruler mark.
#[derive(Clone, Copy, Debug, PartialEq)]
#[rustfmt::skip]
pub struct ChangeTone { pub fg: Hsla, pub bg: Hsla, pub bg_strong: Hsla, pub mark: Hsla }

/// The eight change tones in both modes.
#[derive(Clone, Copy, Debug, PartialEq)]
#[rustfmt::skip]
pub struct ChangeTokenSet { pub light: [ChangeTone; 8], pub dark: [ChangeTone; 8] }

/// The seven syntax roles in both modes, in `kw, ty, fn, num, str, com, attr` order.
#[derive(Clone, Copy, Debug, PartialEq)]
#[rustfmt::skip]
pub struct SyntaxTokenSet { pub light: [Hsla; 7], pub dark: [Hsla; 7] }

impl ChangeTokenSet {
    /// The eight tones of one mode.
    pub fn tones(&self, mode: Mode) -> [ChangeTone; 8] {
        if mode.is_dark() {
            self.dark
        } else {
            self.light
        }
    }

    /// One meaning's tone in one mode.
    pub fn tone(&self, kind: ChangeKind, mode: Mode) -> ChangeTone {
        self.tones(mode)[kind as usize]
    }
}

impl SyntaxTokenSet {
    /// The seven roles of one mode.
    pub fn roles(&self, mode: Mode) -> [Hsla; 7] {
        if mode.is_dark() {
            self.dark
        } else {
            self.light
        }
    }
}

/// The change tones of Violet, Graphite, Ocean and Ember: one meaning never changes colour
/// with the theme. Each row is one meaning's text, line wash and word fill; the mark is the text.
#[rustfmt::skip]
const CHANGE_STANDARD: [[Oklch; 24]; 2] = [
    [(44.,0.114,152.), (95.5,0.036,152.), (88.,0.09,152.),
     (50.,0.19,25.), (95.5,0.022,25.), (88.,0.062,25.),
     (48.5,0.102,75.), (95.5,0.034,75.), (88.,0.084,75.),
     (48.,0.11,240.), (95.5,0.023,240.), (88.,0.064,240.),
     (49.5,0.17,350.), (95.5,0.025,350.), (88.,0.074,350.),
     (43.5,0.082,175.), (95.5,0.029,175.), (88.,0.072,175.),
     (49.5,0.16,320.), (95.5,0.034,320.), (88.,0.094,320.),
     (48.,0.02,265.), (95.5,0.006,265.), (88.,0.01,265.)],
    [(81.5,0.15,152.), (26.,0.045,152.), (38.,0.093,152.),
     (75.5,0.146,25.), (26.,0.057,25.), (38.,0.118,25.),
     (75.5,0.14,75.), (26.,0.042,75.), (38.,0.079,75.),
     (75.,0.13,240.), (26.,0.039,240.), (38.,0.081,240.),
     (76.,0.17,350.), (26.,0.051,350.), (38.,0.105,350.),
     (82.,0.12,175.), (26.,0.036,175.), (38.,0.07,175.),
     (76.,0.16,320.), (26.,0.048,320.), (38.,0.099,320.),
     (75.5,0.02,265.), (26.,0.008,265.), (38.,0.01,265.)],
];

/// High contrast keeps the eight hues but pushes text and fills apart to reach AAA.
#[rustfmt::skip]
const CHANGE_CONTRAST: [[Oklch; 24]; 2] = [
    [(36.5,0.096,152.), (97.,0.036,152.), (90.,0.09,152.),
     (42.5,0.172,25.), (97.,0.014,25.), (90.,0.05,25.),
     (41.,0.086,75.), (97.,0.024,75.), (90.,0.082,75.),
     (40.,0.09,240.), (97.,0.015,240.), (90.,0.054,240.),
     (43.,0.17,350.), (97.,0.017,350.), (90.,0.06,350.),
     (37.,0.068,175.), (97.,0.029,175.), (90.,0.072,175.),
     (42.5,0.16,320.), (97.,0.022,320.), (90.,0.078,320.),
     (40.5,0.02,265.), (97.,0.006,265.), (90.,0.01,265.)],
    [(85.5,0.15,152.), (17.,0.045,152.), (25.,0.065,152.),
     (79.5,0.116,25.), (17.,0.057,25.), (25.,0.1,25.),
     (79.,0.14,75.), (17.,0.036,75.), (25.,0.053,75.),
     (78.,0.124,240.), (17.,0.039,240.), (25.,0.057,240.),
     (80.,0.136,350.), (17.,0.051,350.), (25.,0.103,350.),
     (86.,0.12,175.), (17.,0.032,175.), (25.,0.046,175.),
     (80.,0.16,320.), (17.,0.048,320.), (25.,0.099,320.),
     (78.,0.02,265.), (17.,0.008,265.), (25.,0.01,265.)],
];

/// The syntax roles of the four standard palettes, in `kw, ty, fn, num, str, com, attr` order.
#[rustfmt::skip]
const SYNTAX_STANDARD: [[Oklch; 7]; 2] = [
    [(48.5,0.14,40.), (47.5,0.098,82.), (47.,0.1,240.), (49.,0.13,330.),
     (46.,0.11,150.), (47.5,0.02,60.), (48.,0.09,62.)],
    [(78.,0.132,40.), (76.5,0.1,82.), (76.,0.1,240.), (78.,0.13,330.),
     (75.,0.11,150.), (76.5,0.02,60.), (77.,0.09,62.)],
];

/// High contrast darkens the light roles and lightens the dark ones, for AAA on the card.
#[rustfmt::skip]
const SYNTAX_CONTRAST: [[Oklch; 7]; 2] = [
    [(46.5,0.14,40.), (45.5,0.092,82.), (45.,0.1,240.), (47.,0.13,330.),
     (44.,0.11,150.), (45.5,0.02,60.), (46.,0.09,62.)],
    [(69.5,0.14,40.), (68.5,0.1,82.), (68.,0.1,240.), (70.,0.13,330.),
     (67.,0.11,150.), (68.5,0.02,60.), (68.5,0.09,62.)],
];

/// Reads one mode's 24 ramp entries as eight tones; the mark repeats the text colour.
#[rustfmt::skip]
fn tones(v: [Oklch; 24]) -> [ChangeTone; 8] {
    let c = v.map(oklch);
    std::array::from_fn(|at| ChangeTone {
        fg: c[at * 3], bg: c[at * 3 + 1], bg_strong: c[at * 3 + 2], mark: c[at * 3],
    })
}

/// Reads a light and dark ramp into a change set.
fn change_set([light, dark]: [[Oklch; 24]; 2]) -> ChangeTokenSet {
    ChangeTokenSet {
        light: tones(light),
        dark: tones(dark),
    }
}

/// Reads a light and dark ramp into a syntax set.
fn syntax_set([light, dark]: [[Oklch; 7]; 2]) -> SyntaxTokenSet {
    SyntaxTokenSet {
        light: light.map(oklch),
        dark: dark.map(oklch),
    }
}

/// The change tones of the five palettes, keyed in `PaletteId` order.
#[rustfmt::skip]
pub static CHANGE_TOKENS: LazyLock<[(PaletteId, ChangeTokenSet); 5]> = LazyLock::new(|| [
    (PaletteId::Violet, change_set(CHANGE_STANDARD)),
    (PaletteId::Graphite, change_set(CHANGE_STANDARD)),
    (PaletteId::Ocean, change_set(CHANGE_STANDARD)),
    (PaletteId::Ember, change_set(CHANGE_STANDARD)),
    (PaletteId::Contrast, change_set(CHANGE_CONTRAST)),
]);

/// The syntax roles of the five palettes, keyed in `PaletteId` order.
#[rustfmt::skip]
pub static SYNTAX_TOKENS: LazyLock<[(PaletteId, SyntaxTokenSet); 5]> = LazyLock::new(|| [
    (PaletteId::Violet, syntax_set(SYNTAX_STANDARD)),
    (PaletteId::Graphite, syntax_set(SYNTAX_STANDARD)),
    (PaletteId::Ocean, syntax_set(SYNTAX_STANDARD)),
    (PaletteId::Ember, syntax_set(SYNTAX_STANDARD)),
    (PaletteId::Contrast, syntax_set(SYNTAX_CONTRAST)),
]);

/// The change tones of one palette; the tables are in `PaletteId` order, which the tests pin.
pub fn change_tokens(palette: PaletteId) -> ChangeTokenSet {
    CHANGE_TOKENS[palette as usize].1
}

/// The syntax roles of one palette.
pub fn syntax_tokens(palette: PaletteId) -> SyntaxTokenSet {
    SYNTAX_TOKENS[palette as usize].1
}
