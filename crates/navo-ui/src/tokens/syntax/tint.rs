//! Syntax colours on a tinted diff line: picks a colour that reads on every diff wash.
//! Used by the file diff body and the Stream cards; F3's untinted colours are untouched.

use std::cell::RefCell;
use std::collections::HashMap;

use gpui_kit::Hsla;

use super::{SyntaxTheme, iron, mocha};
use crate::tokens::diff::diff_colors;
use crate::tokens::{Mode, contrast_ratio};

/// The ratio every code colour must reach on every wash (WCAG AA for text).
const MINIMUM: f32 = 4.5;
/// Extra ratio asked of a computed variant, so byte rounding cannot undercut `MINIMUM`.
const MARGIN: f32 = 0.05;
/// How far a variant moves its HSL lightness per try.
const STEP: f32 = 0.01;

/// The theme and mode a colour is resolved for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TintLook {
    pub mode: Mode,
    pub syntax: SyntaxTheme,
}

/// The inputs a tinted colour is resolved from: the role name, the base colour as exact bits,
/// the mode (which fixes the six washes) and the syntax theme (which fixes the comment colour).
type TintKey = (String, [u32; 4], bool, &'static str);

thread_local! {
    /// The tinted colours already resolved on this thread, by their inputs, so a frame that
    /// draws the same roles again does no wash rebuild or contrast search (WR3).
    static TINTS: RefCell<HashMap<TintKey, Hsla>> = RefCell::new(HashMap::new());
}

/// Returns the colour for highlight `name` on a diff line.
///
/// `base` is the colour F3 uses for `name`. On an untinted line (`tinted` false) the result
/// is `base`. On a tinted line, comments (`comment`, `comment.doc`) take `comment_on_tint`
/// of `look.syntax` and `look.mode`. Any other role that misses 4.5:1 on one of the six diff washes
/// of `look.mode` gets `base` with its HSL lightness moved (darker in light mode, lighter in dark
/// mode) until all washes pass; a role that already passes is returned unchanged. The result is
/// memoized by its inputs, so repeated frames reuse it without recomputing (WR3).
pub fn on_tint(name: &str, base: Hsla, tinted: bool, look: TintLook) -> Hsla {
    let TintLook { mode, syntax } = look;
    if !tinted {
        return base;
    }
    let key = (
        name.to_owned(),
        base_bits(base),
        mode.is_dark(),
        syntax.toml(),
    );
    if let Some(colour) = TINTS.with(|cache| cache.borrow().get(&key).copied()) {
        return colour;
    }
    #[cfg(test)]
    SEARCHES.with(|c| c.set(c.get() + 1));
    let colour = resolve(name, base, mode, syntax);
    TINTS.with(|cache| cache.borrow_mut().insert(key, colour));
    colour
}

/// The tinted colour resolved from scratch: the comment colour, or the base moved until it reads
/// on every wash.
fn resolve(name: &str, base: Hsla, mode: Mode, syntax: SyntaxTheme) -> Hsla {
    if name.starts_with("comment") {
        return comment_on_tint(syntax, mode);
    }
    let washes = washes(mode);
    if worst_ratio(base, &washes) >= MINIMUM {
        return base;
    }
    let toward = if mode.is_dark() { STEP } else { -STEP };
    let mut variant = base;
    while worst_ratio(variant, &washes) < MINIMUM + MARGIN && (0.0..1.0).contains(&variant.l) {
        variant.l = (variant.l + toward).clamp(0.0, 1.0);
    }
    variant
}

/// The base colour as exact bits, so equal colours key the cache alike.
fn base_bits(base: Hsla) -> [u32; 4] {
    [
        base.h.to_bits(),
        base.s.to_bits(),
        base.l.to_bits(),
        base.a.to_bits(),
    ]
}

/// The comment colour on a tinted line: the comment hue at light L 0.48 or dark L 0.76.
pub(super) fn comment_on_tint(syntax: SyntaxTheme, mode: Mode) -> Hsla {
    match syntax {
        SyntaxTheme::OxidisedIron => iron::comment_on_tint(mode),
        SyntaxTheme::Mocha => mocha::comment_on_tint(mode),
    }
}

/// The six diff wash colours of `mode`.
fn washes(mode: Mode) -> [Hsla; 6] {
    diff_colors(mode).tones().map(|(_, tone)| tone.wash)
}

/// The lowest contrast ratio of `color` over `washes`.
fn worst_ratio(color: Hsla, washes: &[Hsla]) -> f32 {
    washes
        .iter()
        .map(|wash| contrast_ratio(color, *wash))
        .fold(f32::INFINITY, f32::min)
}

#[cfg(test)]
thread_local! {
    /// How many tinted colours this thread resolved from scratch (test-only).
    static SEARCHES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How many tinted colours this thread resolved from scratch; a memoized frame adds none.
#[cfg(test)]
pub(super) fn searches() -> usize {
    SEARCHES.with(std::cell::Cell::get)
}
