//! The Sidewise theme injector (NC66): one palette and mode projected onto `SidewiseTheme`,
//! `sidewise_kit::tokens::Theme` and the gpui-kit theme, so no projection is left behind.

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::component::highlighter::{HighlightTheme, SyntaxColors};
use gpui_kit::component::theme::{Theme as KitTheme, ThemeColor, ThemeMode, ThemeTokens};
use gpui_kit::{App, Global, Hsla, Rgba, WindowAppearance};
use navo_compare_profile::appearance::PaletteId;
use sidewise_kit::tokens::{
    self, Colors, DiffColors, DiffTone, MarkColors, Mode, Motion, Rows, Surfaces, Theme,
};

use crate::theme_data::change_syntax::{
    ChangeKind, ChangeTokenSet, ChangeTone, SyntaxTokenSet, change_tokens, syntax_tokens,
};
use crate::theme_data::palettes::{
    DIMS, DimTokens, MOTION, ModeTokens, MotionTokens, PALETTES, PaletteSpec,
};

#[path = "theme_gate.rs"]
pub mod gate;

/// The active Sidewise design tokens (NC66): one palette in one mode, with the shared geometry.
#[derive(Clone, Copy, Debug)]
pub struct SidewiseTheme {
    pub palette: PaletteId,
    pub mode: Mode,
    pub spec: PaletteSpec,
    pub dims: DimTokens,
    pub motion: MotionTokens,
    pub change: ChangeTokenSet,
    pub syntax: SyntaxTokenSet,
}

impl Global for SidewiseTheme {}

/// Dark and vibrant dark give `Mode::Dark`; the two light appearances give `Mode::Light`.
pub fn mode_of(appearance: WindowAppearance) -> Mode {
    match appearance {
        WindowAppearance::Dark | WindowAppearance::VibrantDark => Mode::Dark,
        WindowAppearance::Light | WindowAppearance::VibrantLight => Mode::Light,
    }
}

/// Projects one palette and mode onto every theme global the window reads.
///
/// `change` runs first because it loads the mode's registered theme: setting the mode inside
/// `update` would put that load back over these edits. `update` then reconciles `colors` with
/// `tokens`, rebuilds the Base projection and refreshes every window.
pub fn apply_sidewise(palette: PaletteId, mode: Mode, cx: &mut App) {
    let spec = spec_of(palette);
    let tones = mode_tokens(&spec, mode);
    let syntax_set = syntax_tokens(palette);
    let mut theme = navo_theme(palette, mode, &tones);
    theme.syntax = tokens::current(cx).syntax;
    cx.set_global(theme.clone());
    cx.set_global(SidewiseTheme {
        palette,
        mode,
        spec,
        dims: DIMS,
        motion: MOTION,
        change: change_tokens(palette),
        syntax: syntax_set,
    });
    KitTheme::change(kit_mode(mode), None, cx);
    KitTheme::update(cx, move |kit| {
        kit_colors(&tones, &mut kit.colors);
        kit.tokens = ThemeTokens::from(kit.colors);
        kit.radius = DIMS.btn_r;
        kit.radius_lg = DIMS.card_r;
        kit.highlight_theme = Arc::new(highlight(
            &theme,
            &syntax_set.roles(mode),
            &kit.highlight_theme,
        ));
    });
}

/// The spec of one palette; `PALETTES` is in `PaletteId` order, which `theme_bridge_tests` pins.
fn spec_of(palette: PaletteId) -> PaletteSpec {
    PALETTES[palette as usize].1
}

/// The colour tokens of one spec in one mode.
fn mode_tokens(spec: &PaletteSpec, mode: Mode) -> ModeTokens {
    if mode.is_dark() {
        spec.dark
    } else {
        spec.light
    }
}

/// The kit's mode for a token mode.
fn kit_mode(mode: Mode) -> ThemeMode {
    if mode.is_dark() {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    }
}

/// The navo tokens with the Sidewise palette over them, including its change language. The
/// file-kind tones stay navo's: Sidewise has no per-file-type colours of its own.
fn navo_theme(palette: PaletteId, mode: Mode, m: &ModeTokens) -> Theme {
    let mut t = Theme::new(mode);
    let change = change_tokens(palette).tones(mode);
    t.colors = colors(m, &t.colors);
    t.surfaces = surfaces(m);
    t.diff = diff_colors(&change, m);
    t.marks = mark_colors(&change);
    t.radii.md = DIMS.btn_r;
    t.radii.lg = DIMS.row_r;
    t.radii.card = DIMS.card_r;
    t.radii.job_card = DIMS.card_r;
    t.radii.sheet = DIMS.sheet_r;
    t.spacing.row_height = DIMS.row_h;
    t.spacing.rail_width = DIMS.rail_w;
    t.spacing.xl = DIMS.inset_y;
    t.spacing.xxl = DIMS.inset_x;
    t.motion = Motion {
        fade: Duration::from_millis(MOTION.theme_ms),
    };
    t.rows = Rows {
        height: DIMS.row_h,
        inset: DIMS.inset_x,
    };
    t.is_contrast = palette == PaletteId::Contrast;
    t
}

/// The navo colour roles the Sidewise surfaces, text and accent drive.
#[rustfmt::skip]
fn colors(m: &ModeTokens, base: &Colors) -> Colors {
    Colors {
        background: m.surface, surface: m.card, rail: m.surface2, zebra: m.layer2, line: m.sep,
        text: m.text, muted: m.muted, faint: m.faint, hover: m.accent_tint, selection: m.selected,
        on_selection: m.text, on_button: m.on_accent, accent: m.accent, accent_text: m.accent,
        terminal_background: m.surface, terminal_text: m.text, terminal_muted: m.muted,
        ..*base
    }
}

/// The navo surface roles, all solid: NC68 draws no backdrop blur.
#[rustfmt::skip]
fn surfaces(m: &ModeTokens) -> Surfaces {
    Surfaces {
        mica_from: m.surface2, mica_to: m.layer2, card: m.card, card_edge: m.sep,
        card_ring: m.accent_tint, current: m.selected, field: m.surface2, keycap_face: m.card,
        keycap_edge: m.sep, keycap_label: m.text, keycap_lit: m.accent,
        keycap_lit_label: m.on_accent, layer_tint: m.accent_tint, pill: m.card, pill_ring: m.accent,
        pill_label: m.accent, progress_track: m.sep, progress_fill: m.accent, job_card: m.surface,
        sheet: m.card, scrim: m.scrim, tile: m.card, tile_edge: m.sep,
    }
}

/// The kit colour roles the Sidewise palette drives; `tokens` is rebuilt from these, so the
/// renderable backgrounds and the Base scrollbar follow the same colours.
#[rustfmt::skip]
fn kit_colors(m: &ModeTokens, k: &mut ThemeColor) {
    k.background = m.surface; k.foreground = m.text; k.border = m.sep; k.input = m.sep;
    k.ring = m.accent; k.muted = m.layer2; k.muted_foreground = m.muted;
    k.accent = m.accent_tint; k.accent_foreground = m.text; k.selection = m.selected;
    k.overlay = m.scrim; k.primary = m.accent; k.primary_hover = m.accent;
    k.primary_active = m.accent; k.primary_foreground = m.on_accent;
    k.button_primary = m.accent; k.button_primary_hover = m.accent;
    k.button_primary_active = m.accent; k.button_primary_foreground = m.on_accent;
    k.button = m.card; k.button_hover = m.accent_tint; k.button_active = m.selected;
    k.button_foreground = m.text; k.secondary = m.card; k.secondary_hover = m.accent_tint;
    k.secondary_active = m.selected; k.secondary_foreground = m.text; k.progress_bar = m.accent;
    k.scrollbar = m.surface2; k.scrollbar_thumb = m.sep_strong; k.scrollbar_thumb_hover = m.faint;
    k.popover = m.card; k.popover_foreground = m.text; k.tab_bar = m.surface2; k.tab = m.surface2;
    k.tab_active = m.card; k.tab_foreground = m.muted; k.tab_active_foreground = m.text;
    k.list = m.card; k.list_hover = m.accent_tint; k.list_active = m.selected;
    k.list_even = m.layer2; k.sidebar = m.surface2; k.sidebar_foreground = m.text;
    k.sidebar_border = m.sep; k.title_bar = m.surface2; k.title_bar_border = m.sep;
    k.status_bar = m.surface2; k.status_bar_border = m.sep; k.window_border = m.sep;
    k.drag_border = m.accent;
}

/// The kit highlighter names of the seven Sidewise roles, in `SyntaxTokenSet` order; navo's
/// constant and boolean share the number role, and its doc comments share the comment role.
#[rustfmt::skip]
const ROLE_NAMES: [(&str, usize); 10] = [
    ("keyword", 0), ("type", 1), ("function", 2), ("number", 3), ("constant", 3), ("boolean", 3),
    ("string", 4), ("comment", 5), ("comment_doc", 5), ("attribute", 6),
];

/// The six navo diff tones from the Sidewise change language. `same` is the neutral tone navo
/// calls auto-merged, and the gap hatch is the prototype's `--hatch: var(--layer2)`.
#[rustfmt::skip]
fn diff_colors(t: &[ChangeTone; 8], m: &ModeTokens) -> DiffColors {
    let tone = |kind: ChangeKind| {
        let c = t[kind as usize];
        DiffTone { text: c.fg, mark: c.mark, fill: c.bg_strong, wash: c.bg }
    };
    DiffColors {
        delete: tone(ChangeKind::Del), add: tone(ChangeKind::Add), change: tone(ChangeKind::Chg),
        moved: tone(ChangeKind::Mov), conflict: tone(ChangeKind::Cfl),
        auto_merged: tone(ChangeKind::Same), gap_hatch: m.layer2,
    }
}

/// The change-map fills and ticks: the eight line washes and their marks, in meaning order.
fn mark_colors(t: &[ChangeTone; 8]) -> MarkColors {
    MarkColors {
        fills: t.map(|c| c.bg),
        ticks: t.map(|c| c.mark),
    }
}

/// The kit highlight theme with the Sidewise roles over navo's editor colours and fallbacks.
#[rustfmt::skip]
fn highlight(theme: &Theme, roles: &[Hsla; 7], base: &HighlightTheme) -> HighlightTheme {
    let mut out = tokens::highlight_theme(theme.syntax, theme.mode, &theme.colors, base);
    if let Some(s) = sidewise_roles(roles) {
        let syn = &mut out.style.syntax;
        syn.keyword = s.keyword; syn.type_ = s.type_; syn.function = s.function;
        syn.number = s.number; syn.constant = s.constant; syn.boolean = s.boolean;
        syn.string = s.string; syn.comment = s.comment; syn.comment_doc = s.comment_doc;
        syn.attribute = s.attribute;
    }
    out
}

/// The Sidewise roles as kit syntax styles, through the same TOML path navo's own themes use:
/// `ThemeStyle` has no public constructor, so a style can only be read back from a table.
fn sidewise_roles(roles: &[Hsla; 7]) -> Option<SyntaxColors> {
    let mut src = String::new();
    for (name, at) in ROLE_NAMES {
        let rgb = Rgba::from(roles[at]);
        let byte = |c: f32| (c * 255.0).round() as u8;
        let hex = format!("#{:02x}{:02x}{:02x}", byte(rgb.r), byte(rgb.g), byte(rgb.b));
        src.push_str(&format!("{name}.color = \"{hex}\"\n"));
    }
    toml::from_str(&src).ok()
}
