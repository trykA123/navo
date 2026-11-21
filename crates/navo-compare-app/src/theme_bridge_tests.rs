//! Tests of the Sidewise injector (NC66): one palette and mode reach every theme projection,
//! and a switch leaves no colour of the previous palette or mode behind.

use std::time::Duration;

use gpui_kit::component::theme::{Theme as KitTheme, ThemeMode};
use gpui_kit::{App, Hsla, ReadGlobal, Rgba, TestAppContext};
use navo_compare_profile::appearance::PaletteId;
use sidewise_kit::tokens::{self, Mode};

use crate::theme::{SidewiseTheme, apply_sidewise};
use crate::theme_data::change_syntax::{ChangeKind, change_tokens, syntax_tokens};
use crate::theme_data::palettes::{DIMS, MOTION, ModeTokens, PALETTES, PaletteSpec};

/// The kit highlighter names of the seven Sidewise syntax roles, in role order.
const ROLE_NAMES: [&str; 7] = [
    "keyword",
    "type",
    "function",
    "number",
    "string",
    "comment",
    "attribute",
];

/// The static spec of one palette, found by id so the tests do not lean on table order.
fn spec(palette: PaletteId) -> PaletteSpec {
    PALETTES
        .iter()
        .find(|(id, _)| *id == palette)
        .map(|(_, spec)| *spec)
        .expect("every PaletteId has a spec")
}

/// A kit app with the injector applied once; later switches in the same test skip the init.
fn themed(cx: &mut TestAppContext, palette: PaletteId, mode: Mode) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        apply_sidewise(palette, mode, cx);
    });
}

/// The navo tokens carry the Sidewise colours, geometry and motion of `m`.
fn assert_navo(navo: &tokens::Theme, m: &ModeTokens, mode: Mode) {
    assert_eq!(navo.mode, mode, "navo mode");
    assert_eq!(navo.colors.background, m.surface, "navo background");
    assert_eq!(navo.colors.surface, m.card, "navo surface");
    assert_eq!(navo.colors.text, m.text, "navo text");
    assert_eq!(navo.colors.accent, m.accent, "navo accent");
    assert_eq!(navo.colors.selection, m.selected, "navo selection");
    assert_eq!(navo.surfaces.card, m.card, "navo card surface");
    assert_eq!(navo.surfaces.scrim, m.scrim, "navo scrim");
    assert_eq!(navo.radii.card, DIMS.card_r, "navo card radius");
    assert_eq!(navo.radii.sheet, DIMS.sheet_r, "navo sheet radius");
    assert_eq!(navo.rows.height, DIMS.row_h, "navo row height");
    assert_eq!(navo.spacing.rail_width, DIMS.rail_w, "navo rail width");
    assert_eq!(
        navo.motion.fade,
        Duration::from_millis(MOTION.theme_ms),
        "navo theme fade"
    );
}

/// The kit colours, resolved tokens and radii carry the Sidewise palette of `m`.
fn assert_kit(kit: &KitTheme, m: &ModeTokens, mode: ThemeMode) {
    assert_eq!(kit.mode, mode, "kit mode");
    assert_eq!(kit.colors.background, m.surface, "kit background");
    assert_eq!(kit.colors.foreground, m.text, "kit foreground");
    assert_eq!(kit.colors.primary, m.accent, "kit primary");
    assert_eq!(
        kit.tokens.background.color, m.surface,
        "kit token background"
    );
    assert_eq!(kit.tokens.foreground.color, m.text, "kit token foreground");
    assert_eq!(
        kit.tokens.scrollbar_thumb.color, m.sep_strong,
        "kit token scrollbar thumb"
    );
    assert_eq!(kit.tokens.accent.color, m.accent_tint, "kit token accent");
    assert_eq!(kit.radius, DIMS.btn_r, "kit radius");
    assert_eq!(kit.radius_lg, DIMS.card_r, "kit large radius");
}

/// The Base projection and the highlight theme follow the kit's colours and mode.
fn assert_base_and_highlight(cx: &App, m: &ModeTokens, mode: ThemeMode) {
    let kit = KitTheme::global(cx);
    let base = gpui_kit::base::Theme::global(cx);
    let appearance = if mode == ThemeMode::Dark {
        gpui_kit::base::ThemeAppearance::Dark
    } else {
        gpui_kit::base::ThemeAppearance::Light
    };
    assert_eq!(base.appearance, appearance, "base appearance");
    assert_eq!(base.tokens.colors.background, m.surface, "base background");
    assert_eq!(base.tokens.colors.ring, m.accent, "base ring");
    assert_eq!(base.tokens.radius.md, DIMS.btn_r, "base radius");
    assert_eq!(base.tokens.radius.lg, DIMS.card_r, "base large radius");
    assert_eq!(
        base.scrollbar.mode(),
        kit.scrollbar_mode,
        "base scrollbar mode"
    );
    assert_eq!(kit.highlight_theme.appearance, mode, "highlight appearance");
    assert_eq!(
        kit.highlight_theme.style.editor_background,
        Some(m.surface),
        "highlight editor background"
    );
    assert_eq!(
        kit.highlight_theme.style.editor_foreground,
        Some(m.text),
        "highlight editor foreground"
    );
}

/// Violet Light reaches the navo tokens, the kit colours and tokens, Base and the highlight theme.
#[gpui_kit::test]
fn violet_light_projects_onto_every_theme_global(cx: &mut TestAppContext) {
    themed(cx, PaletteId::Violet, Mode::Light);
    cx.update(|cx| {
        let m = spec(PaletteId::Violet).light;
        assert_navo(tokens::current(cx), &m, Mode::Light);
        assert_kit(KitTheme::global(cx), &m, ThemeMode::Light);
        assert_base_and_highlight(cx, &m, ThemeMode::Light);
        assert!(
            !tokens::current(cx).is_contrast,
            "violet is not the high contrast palette"
        );
        let side = SidewiseTheme::global(cx);
        assert_eq!(side.palette, PaletteId::Violet, "sidewise palette");
        assert_eq!(side.mode, Mode::Light, "sidewise mode");
        assert_eq!(side.spec.ring_px, 2.0, "sidewise focus ring");
        assert_eq!(
            side.dims.sidebar_w, DIMS.sidebar_w,
            "sidewise sidebar width"
        );
        assert_eq!(
            side.motion.side_ms, MOTION.side_ms,
            "sidewise sidebar motion"
        );
    });
}

/// Ember Dark after Violet Light leaves no light colour in any projection.
#[gpui_kit::test]
fn ember_dark_replaces_every_light_projection(cx: &mut TestAppContext) {
    themed(cx, PaletteId::Violet, Mode::Light);
    let light = spec(PaletteId::Violet).light;
    cx.update(|cx| apply_sidewise(PaletteId::Ember, Mode::Dark, cx));
    cx.update(|cx| {
        let m = spec(PaletteId::Ember).dark;
        assert_navo(tokens::current(cx), &m, Mode::Dark);
        assert_kit(KitTheme::global(cx), &m, ThemeMode::Dark);
        assert_base_and_highlight(cx, &m, ThemeMode::Dark);
        let navo = tokens::current(cx);
        let kit = KitTheme::global(cx);
        assert_ne!(
            navo.colors.background, light.surface,
            "navo kept the light surface"
        );
        assert_ne!(
            kit.tokens.background.color, light.surface,
            "stale light token"
        );
        assert_ne!(
            kit.tokens.foreground.color, light.text,
            "stale light token text"
        );
        let side = SidewiseTheme::global(cx);
        assert_eq!(
            side.palette,
            PaletteId::Ember,
            "sidewise palette after the switch"
        );
        assert_eq!(side.mode, Mode::Dark, "sidewise mode after the switch");
    });
}

/// Contrast Light after a dark palette flags high contrast and thickens the focus ring.
#[gpui_kit::test]
fn contrast_light_flags_contrast_and_clears_the_dark_tokens(cx: &mut TestAppContext) {
    themed(cx, PaletteId::Ember, Mode::Dark);
    let dark = spec(PaletteId::Ember).dark;
    cx.update(|cx| apply_sidewise(PaletteId::Contrast, Mode::Light, cx));
    cx.update(|cx| {
        let m = spec(PaletteId::Contrast).light;
        assert_navo(tokens::current(cx), &m, Mode::Light);
        assert_kit(KitTheme::global(cx), &m, ThemeMode::Light);
        assert_base_and_highlight(cx, &m, ThemeMode::Light);
        assert!(
            tokens::current(cx).is_contrast,
            "the contrast palette must flag high contrast"
        );
        let kit = KitTheme::global(cx);
        assert_ne!(
            kit.tokens.background.color, dark.surface,
            "stale dark token"
        );
        assert_eq!(
            SidewiseTheme::global(cx).spec.ring_px,
            3.0,
            "contrast focus ring"
        );
    });
}

/// The projected colours are Sidewise's, not the navo palette the injector started from.
#[gpui_kit::test]
fn the_projection_is_not_the_navo_palette(cx: &mut TestAppContext) {
    themed(cx, PaletteId::Ocean, Mode::Dark);
    cx.update(|cx| {
        let baseline = tokens::Theme::new(Mode::Dark);
        let m = spec(PaletteId::Ocean).dark;
        let navo = tokens::current(cx);
        assert_eq!(navo.colors.background, m.surface, "navo background");
        assert_ne!(
            navo.colors.background, baseline.colors.background,
            "navo kept its own palette"
        );
        assert_ne!(
            KitTheme::global(cx).colors.background,
            baseline.colors.background,
            "the kit kept the navo palette"
        );
    });
}

/// Applying the same palette and mode twice changes nothing.
#[gpui_kit::test]
fn applying_the_same_palette_twice_is_idempotent(cx: &mut TestAppContext) {
    themed(cx, PaletteId::Graphite, Mode::Light);
    let first = cx.update(|cx| (tokens::current(cx).clone(), KitTheme::global(cx).colors));
    cx.update(|cx| apply_sidewise(PaletteId::Graphite, Mode::Light, cx));
    cx.update(|cx| {
        assert_eq!(tokens::current(cx), &first.0, "navo tokens drifted");
        assert_eq!(KitTheme::global(cx).colors, first.1, "kit colours drifted");
    });
}

/// `PALETTES` is keyed in `PaletteId` order, so the injector can index it by discriminant.
#[test]
fn palettes_are_in_palette_id_order() {
    for (at, (id, _)) in PALETTES.iter().enumerate() {
        assert_eq!(*id as usize, at, "{id:?} is not at its discriminant index");
    }
}

/// Contrast Dark after Violet Light carries the Sidewise change tones and syntax roles,
/// not navo's mode-derived defaults.
#[gpui_kit::test]
fn contrast_dark_carries_the_sidewise_change_and_syntax_tokens(cx: &mut TestAppContext) {
    themed(cx, PaletteId::Violet, Mode::Light);
    cx.update(|cx| apply_sidewise(PaletteId::Contrast, Mode::Dark, cx));
    cx.update(|cx| {
        let tones = change_tokens(PaletteId::Contrast).tones(Mode::Dark);
        let roles = syntax_tokens(PaletteId::Contrast).roles(Mode::Dark);
        let tone = |kind: ChangeKind| tones[kind as usize];
        let (add, del, chg) = (
            tone(ChangeKind::Add),
            tone(ChangeKind::Del),
            tone(ChangeKind::Chg),
        );
        let navo = tokens::current(cx);
        assert_eq!(navo.diff.add.text, add.fg, "diff add text");
        assert_eq!(navo.diff.add.wash, add.bg, "diff add line wash");
        assert_eq!(
            navo.diff.delete.fill, del.bg_strong,
            "diff delete word fill"
        );
        assert_eq!(navo.diff.change.mark, chg.mark, "diff change mark");
        assert_eq!(
            navo.diff.moved.text,
            tone(ChangeKind::Mov).fg,
            "diff moved text"
        );
        assert_eq!(
            navo.diff.conflict.text,
            tone(ChangeKind::Cfl).fg,
            "diff conflict text"
        );
        assert_eq!(navo.marks.fills, tones.map(|t| t.bg), "change map fills");
        assert_eq!(navo.marks.ticks, tones.map(|t| t.mark), "change map ticks");
        let side = SidewiseTheme::global(cx);
        assert_eq!(
            side.change,
            change_tokens(PaletteId::Contrast),
            "sidewise change set"
        );
        assert_eq!(
            side.syntax,
            syntax_tokens(PaletteId::Contrast),
            "sidewise syntax set"
        );
        let kit = KitTheme::global(cx);
        assert_syntax_roles(kit, &roles);
    });
}

/// The kit keeps syntax colours as 8-bit hex in both serde directions, so a role is the token
/// quantised to a byte per channel — not the raw OKLCH conversion.
fn quantised(color: Hsla) -> Hsla {
    let rgb = Rgba::from(color);
    let byte = |c: f32| (c * 255.0).round() / 255.0;
    Rgba {
        r: byte(rgb.r),
        g: byte(rgb.g),
        b: byte(rgb.b),
        a: 1.0,
    }
    .into()
}

/// Every kit syntax role carries its own Sidewise token.
fn assert_syntax_roles(kit: &KitTheme, roles: &[Hsla; 7]) {
    for (at, name) in ROLE_NAMES.iter().enumerate() {
        let got = kit
            .highlight_theme
            .style(name)
            .and_then(|style| style.color);
        assert_eq!(got, Some(quantised(roles[at])), "syntax {name}");
    }
}
