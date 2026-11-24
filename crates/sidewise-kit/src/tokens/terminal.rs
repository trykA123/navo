//! Terminal colour group: one `terminal.*` palette for Follow Navo and the imported schemes.
//!
//! A scheme is data (OKLCH triples) and every derived token is computed here. The optional
//! "keep readable" lift moves OKLCH lightness only, until text reaches AA against the scheme
//! background and the block tints.

use gpui_kit::component::theme::oklch;
use gpui_kit::{Hsla, Rgba};

use super::terminal_schemes::{SchemePalette, TerminalScheme};
use super::{Colors, Mode, contrast_ratio};

/// Text must reach this ratio against the background and both block tints.
const READABLE: f32 = 4.6;

/// Terminal header height, in points.
pub const HEADER_HEIGHT: f32 = 36.0;
/// Terminal gutter width, in points.
pub const GUTTER: f32 = 22.0;
/// Terminal ruler width, in points.
pub const RULER: f32 = 10.0;
/// Side-panel terminal width, in points.
pub const SIDE_WIDTH: f32 = 440.0;

/// Every colour the terminal draws beyond the seeded background, text and muted tones.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TerminalColors {
    /// Scheme background; Follow Navo copies [`Colors::terminal_background`].
    pub background: Hsla,
    /// Default text; Follow Navo copies [`Colors::terminal_text`].
    pub text: Hsla,
    /// Dim text; Follow Navo copies [`Colors::terminal_muted`].
    pub muted: Hsla,
    /// Cursor block, bar or underline.
    pub cursor: Hsla,
    /// Selection fill; the grid draws text over it.
    pub selection: Hsla,
    /// The 16 ANSI colours, normal then bright.
    pub ansi: [Hsla; 16],
    /// Idle prompt mark.
    pub mark_ok: Hsla,
    /// Failed command mark and its exit chip.
    pub mark_fail: Hsla,
    /// Running command mark and idle prompt caret.
    pub mark_run: Hsla,
    /// Hovered command block.
    pub block_hover: Hsla,
    /// Current command block.
    pub block_current: Hsla,
    /// Find-in-scrollback match fill.
    pub find_match: Hsla,
    /// Text drawn on a find match.
    pub find_match_text: Hsla,
    /// Current find match and its ruler tick.
    pub find_current: Hsla,
    /// Ring around the current find match.
    pub find_ring: Hsla,
    /// Header strip and overlays inside the terminal.
    pub chrome: Hsla,
}

impl TerminalColors {
    /// The default "Follow Navo theme" group, seeded from the live theme tokens.
    pub fn follow(mode: Mode, colors: &Colors) -> Self {
        Self::navo(mode, colors).finish(true)
    }

    /// A scheme's group. [`TerminalScheme::Navo`] follows the live theme; the imported schemes
    /// are fixed data, with Catppuccin picking Latte or Mocha by mode.
    pub fn from_scheme(
        scheme: TerminalScheme,
        mode: Mode,
        keep_readable: bool,
        colors: &Colors,
    ) -> Self {
        match scheme.palette(mode) {
            Some(palette) => Seed::from(*palette).finish(keep_readable),
            None => Self::navo(mode, colors).finish(keep_readable),
        }
    }

    fn navo(mode: Mode, colors: &Colors) -> Seed {
        let text = Oklch::from_hsla(colors.terminal_text);
        let muted = Oklch::from_hsla(colors.terminal_muted);
        Seed {
            background: Oklch::from_hsla(colors.terminal_background),
            text,
            muted,
            cursor: Oklch::from_hsla(colors.accent),
            selection: Oklch::from_hsla(colors.selection),
            ansi: navo_ansi(mode, text, muted),
            mark_fail: Oklch::from_hsla(colors.mark),
            mark_run: Oklch::from_hsla(colors.accent),
            find_current: Oklch::from_hsla(colors.warn),
            chrome: Oklch::from_hsla(colors.background),
        }
    }
}

/// One OKLCH colour. Lightness and chroma are 0..1 and 0..0.37; hue is in degrees.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct Oklch {
    pub l: f32,
    pub c: f32,
    pub h: f32,
}

impl Oklch {
    pub(super) const fn new(l: f32, c: f32, h: f32) -> Self {
        Self { l, c, h }
    }

    fn color(self) -> Hsla {
        oklch(self.l, self.c, self.h)
    }

    /// Mixes lightness and chroma linearly and hue along the shortest arc.
    fn mix(self, other: Self, amount: f32) -> Self {
        let turn = (other.h - self.h + 540.0).rem_euclid(360.0) - 180.0;
        Self {
            l: self.l + (other.l - self.l) * amount,
            c: self.c + (other.c - self.c) * amount,
            h: (self.h + turn * amount).rem_euclid(360.0),
        }
    }

    fn shifted(self, delta: f32) -> Self {
        Self {
            l: (self.l + delta).clamp(0.0, 1.0),
            ..self
        }
    }

    fn from_hsla(color: Hsla) -> Self {
        let rgb = Rgba::from(color);
        let linear = |channel: f32| {
            if channel <= 0.040_45 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        };
        let r = linear(rgb.r);
        let g = linear(rgb.g);
        let b = linear(rgb.b);
        let long = 0.412_221_5 * r + 0.536_332_5 * g + 0.051_446 * b;
        let medium = 0.211_903_5 * r + 0.680_699_5 * g + 0.107_397 * b;
        let short = 0.088_302_5 * r + 0.281_718_8 * g + 0.629_978_7 * b;
        let long = long.cbrt();
        let medium = medium.cbrt();
        let short = short.cbrt();
        let lightness = 0.210_454_3 * long + 0.793_617_8 * medium - 0.004_072 * short;
        let a = 1.977_998_5 * long - 2.428_592_2 * medium + 0.450_593_7 * short;
        let b = 0.025_904 * long + 0.782_771_8 * medium - 0.808_675_8 * short;
        Self {
            l: lightness,
            c: a.hypot(b),
            h: b.atan2(a).to_degrees().rem_euclid(360.0),
        }
    }
}

/// A scheme's raw tokens before the block, find and mark values are derived.
struct Seed {
    background: Oklch,
    text: Oklch,
    muted: Oklch,
    cursor: Oklch,
    selection: Oklch,
    ansi: [Oklch; 16],
    mark_fail: Oklch,
    mark_run: Oklch,
    find_current: Oklch,
    chrome: Oklch,
}

impl From<SchemePalette> for Seed {
    fn from(palette: SchemePalette) -> Self {
        let dark = palette.background.l < 0.5;
        Self {
            background: palette.background,
            text: palette.text,
            muted: palette.ansi[8],
            cursor: palette.cursor,
            selection: palette.selection,
            ansi: palette.ansi,
            mark_fail: palette.ansi[1],
            mark_run: palette.ansi[4],
            find_current: palette.ansi[3],
            chrome: palette.background.shifted(if dark { -0.03 } else { 0.017 }),
        }
    }
}

impl Seed {
    fn finish(self, keep_readable: bool) -> TerminalColors {
        let block_hover = self.background.mix(self.text, 0.04);
        let block_current = self.background.mix(self.cursor, 0.07);
        let text_grounds = [self.background, block_hover, block_current, self.selection];
        let block_grounds = [self.background, block_hover, block_current];
        let lift = |color: Oklch, grounds: &[Oklch], minimum: f32| {
            if keep_readable {
                readable(color, grounds, minimum)
            } else {
                color
            }
        };
        let text = lift(self.text, &text_grounds, READABLE);
        let muted = lift(self.muted, &text_grounds, READABLE);
        let ansi = self.ansi.map(|color| lift(color, &text_grounds, READABLE));
        let cursor = lift(self.cursor, &block_grounds, 3.0);
        let mark_fail = lift(self.mark_fail, &block_grounds, 3.0);
        let mark_run = lift(self.mark_run, &block_grounds, 3.0);
        let find_current = lift(self.find_current, &[self.background], READABLE);
        TerminalColors {
            background: self.background.color(),
            text: text.color(),
            muted: muted.color(),
            cursor: cursor.color(),
            selection: self.selection.color(),
            ansi: ansi.map(Oklch::color),
            mark_ok: ansi[8].color(),
            mark_fail: mark_fail.color(),
            mark_run: mark_run.color(),
            block_hover: block_hover.color(),
            block_current: block_current.color(),
            find_match: find_current.color(),
            find_match_text: self.background.color(),
            find_current: find_current.color(),
            find_ring: text.color(),
            chrome: self.chrome.color(),
        }
    }
}

/// Moves OKLCH lightness away from the background until the colour reads on every ground.
fn readable(color: Oklch, grounds: &[Oklch], minimum: f32) -> Oklch {
    if satisfies(color, grounds, minimum) {
        return color;
    }
    let step = if grounds[0].l < 0.5 { 0.01 } else { -0.01 };
    let mut candidate = color;
    while (0.0..=1.0).contains(&candidate.l) {
        candidate = candidate.shifted(step);
        if satisfies(candidate, grounds, minimum) {
            return candidate;
        }
        if candidate.l == 0.0 || candidate.l == 1.0 {
            break;
        }
    }
    candidate
}

fn satisfies(color: Oklch, grounds: &[Oklch], minimum: f32) -> bool {
    let color = color.color();
    grounds
        .iter()
        .all(|ground| contrast_ratio(color, ground.color()) >= minimum)
}

/// Follow Navo's ANSI ramp: theme hues at fixed lightness, with text and muted filling 0/7/8/15.
fn navo_ansi(mode: Mode, text: Oklch, muted: Oklch) -> [Oklch; 16] {
    if mode.is_dark() {
        [
            text,
            Oklch::new(0.76, 0.13, 25.0),
            Oklch::new(0.78, 0.12, 150.0),
            Oklch::new(0.82, 0.11, 85.0),
            Oklch::new(0.76, 0.11, 250.0),
            Oklch::new(0.76, 0.12, 330.0),
            Oklch::new(0.78, 0.09, 200.0),
            muted,
            muted,
            Oklch::new(0.82, 0.12, 25.0),
            Oklch::new(0.84, 0.11, 150.0),
            Oklch::new(0.88, 0.10, 85.0),
            Oklch::new(0.82, 0.10, 250.0),
            Oklch::new(0.82, 0.11, 330.0),
            Oklch::new(0.84, 0.08, 200.0),
            text,
        ]
    } else {
        [
            text,
            Oklch::new(0.48, 0.15, 25.0),
            Oklch::new(0.48, 0.12, 150.0),
            Oklch::new(0.48, 0.10, 75.0),
            Oklch::new(0.48, 0.13, 250.0),
            Oklch::new(0.48, 0.14, 330.0),
            Oklch::new(0.48, 0.08, 200.0),
            muted,
            muted,
            Oklch::new(0.45, 0.16, 25.0),
            Oklch::new(0.45, 0.13, 150.0),
            Oklch::new(0.45, 0.11, 75.0),
            Oklch::new(0.45, 0.14, 250.0),
            Oklch::new(0.45, 0.15, 330.0),
            Oklch::new(0.45, 0.09, 200.0),
            text,
        ]
    }
}
