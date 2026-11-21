//! The whole `contrast.md` audit (NC66): all 2,010 measured pairs, five palettes in two modes,
//! each at the gate the audit gives it — 4.5 body, 7.0 on high contrast, 3.0 for UI parts.

use gpui_kit::Hsla;
use navo_compare_profile::appearance::PaletteId;
use sidewise_kit::tokens::{Mode, contrast_ratio, relative_luminance};

use super::change_syntax::{
    CHANGE_TOKENS, ChangeKind, ChangeTone, SYNTAX_TOKENS, change_tokens, syntax_tokens,
};
use super::palettes::{ModeTokens, PALETTES};

/// Pairs per palette and mode, as `contrast.md` counts them.
const PER_THEME: usize = 201;
/// The whole audit: five palettes in two modes.
const TOTAL: usize = 2010;
/// Meaning-carrying UI parts: the focus ring, strong separators, gutter and ruler marks.
const UI: f32 = 3.0;
/// Body text, change text and syntax text.
const TEXT: f32 = 4.5;
/// WCAG AAA, which the high contrast palette gates body text at.
const AAA: f32 = 7.0;

/// The eight change meanings, in tone order.
const CHANGE_NAMES: [&str; 8] = ["add", "del", "chg", "mov", "ol", "or", "cfl", "same"];
/// The seven syntax roles, in role order.
const SYNTAX_NAMES: [&str; 7] = ["kw", "ty", "fn", "num", "str", "com", "attr"];

/// One measured pair and the gate it must clear.
struct Pair {
    name: String,
    fg: Hsla,
    bg: Hsla,
    gate: f32,
}

impl Pair {
    fn new(name: String, fg: Hsla, bg: Hsla, gate: f32) -> Self {
        Self { name, fg, bg, gate }
    }
}

/// The six surfaces body text is measured on, in audit order.
#[rustfmt::skip]
fn surfaces(m: &ModeTokens) -> [(&'static str, Hsla); 6] {
    [("bg", m.surface), ("bg2", m.surface2), ("card", m.card), ("layer2", m.layer2),
     ("selected", m.selected), ("accent-tint", m.accent_tint)]
}

/// The 20 text and accent pairs: six surfaces, the line-number faint and the accent fill.
fn text_pairs(m: &ModeTokens, body: f32) -> Vec<Pair> {
    let mut out = Vec::new();
    for (name, bg) in surfaces(m) {
        out.push(Pair::new(format!("text on {name}"), m.text, bg, body));
        out.push(Pair::new(format!("muted on {name}"), m.muted, bg, body));
    }
    for (name, bg) in [("card", m.card), ("bg2", m.surface2), ("bg", m.surface)] {
        out.push(Pair::new(format!("faint on {name}"), m.faint, bg, body));
    }
    out.push(Pair::new(
        "on-accent on accent".into(),
        m.on_accent,
        m.accent,
        body,
    ));
    let accents = [
        ("card", m.card),
        ("bg", m.surface),
        ("accent-tint", m.accent_tint),
        ("selected", m.selected),
    ];
    for (name, bg) in accents {
        out.push(Pair::new(format!("accent on {name}"), m.accent, bg, body));
    }
    out
}

/// The meaning-carrying UI parts and the two toast pairs.
fn ui_pairs(m: &ModeTokens, body: f32) -> Vec<Pair> {
    let mut out = Vec::new();
    for (name, bg) in [("bg", m.surface), ("card", m.card)] {
        out.push(Pair::new(format!("focus ring on {name}"), m.accent, bg, UI));
    }
    for (name, bg) in [("card", m.card), ("bg2", m.surface2)] {
        out.push(Pair::new(
            format!("sep strong on {name}"),
            m.sep_strong,
            bg,
            UI,
        ));
    }
    out.push(Pair::new(
        "toast text on toast".into(),
        m.toast[1],
        m.toast[0],
        body,
    ));
    out.push(Pair::new(
        "toast action on toast".into(),
        m.toast[2],
        m.toast[0],
        body,
    ));
    out
}

/// Seven pairs per meaning: four row surfaces, its line wash, its word fill and its mark.
fn change_pairs(m: &ModeTokens, tones: &[ChangeTone; 8], body: f32) -> Vec<Pair> {
    let mut out = Vec::new();
    for (at, tone) in tones.iter().enumerate() {
        let name = CHANGE_NAMES[at];
        let rows = [
            ("card", m.card),
            ("bg2", m.surface2),
            ("a selected row", m.selected),
            ("a hovered row", m.layer2),
        ];
        for (where_, bg) in rows {
            out.push(Pair::new(
                format!("{name} text on {where_}"),
                tone.fg,
                bg,
                body,
            ));
        }
        out.push(Pair::new(
            format!("{name} text on its line"),
            tone.fg,
            tone.bg,
            TEXT,
        ));
        out.push(Pair::new(
            format!("{name} text on its word fill"),
            tone.fg,
            tone.bg_strong,
            TEXT,
        ));
        out.push(Pair::new(
            format!("{name} mark on bg2"),
            tone.mark,
            m.surface2,
            UI,
        ));
    }
    out
}

/// Seventeen pairs per role: the card, then every change line wash and word fill.
fn syntax_pairs(
    m: &ModeTokens,
    tones: &[ChangeTone; 8],
    roles: &[Hsla; 7],
    body: f32,
) -> Vec<Pair> {
    let mut out = Vec::new();
    for (at, color) in roles.iter().enumerate() {
        let name = SYNTAX_NAMES[at];
        out.push(Pair::new(
            format!("syntax {name} on card"),
            *color,
            m.card,
            body,
        ));
        for (tone_at, tone) in tones.iter().enumerate() {
            let change = CHANGE_NAMES[tone_at];
            out.push(Pair::new(
                format!("syntax {name} on {change} line"),
                *color,
                tone.bg,
                TEXT,
            ));
            out.push(Pair::new(
                format!("syntax {name} on {change} word"),
                *color,
                tone.bg_strong,
                TEXT,
            ));
        }
    }
    out
}

/// Every pair of one palette in one mode, in audit order.
fn matrix(palette: PaletteId, mode: Mode) -> Vec<Pair> {
    let spec = PALETTES[palette as usize].1;
    let m = if mode.is_dark() {
        spec.dark
    } else {
        spec.light
    };
    let tones = change_tokens(palette).tones(mode);
    let roles = syntax_tokens(palette).roles(mode);
    let body = if palette == PaletteId::Contrast {
        AAA
    } else {
        TEXT
    };
    let mut out = text_pairs(&m, body);
    out.extend(ui_pairs(&m, body));
    out.extend(change_pairs(&m, &tones, body));
    out.extend(syntax_pairs(&m, &tones, &roles, body));
    out
}

/// All 2,010 pairs of `contrast.md` clear their gate in every palette and mode.
#[test]
fn every_pair_of_the_audit_passes() {
    let mut checked = 0;
    for (palette, _) in PALETTES.iter() {
        for mode in [Mode::Light, Mode::Dark] {
            for pair in matrix(*palette, mode) {
                let ratio = contrast_ratio(pair.fg, pair.bg);
                assert!(
                    ratio >= pair.gate,
                    "{palette:?} {mode:?} {}: {ratio:.2} < {}",
                    pair.name,
                    pair.gate
                );
                checked += 1;
            }
        }
    }
    assert_eq!(checked, TOTAL, "the audit must cover every pair");
}

/// Each of the ten themes contributes exactly the 201 pairs the audit lists.
#[test]
fn every_theme_has_the_201_pairs_of_the_audit() {
    for (palette, _) in PALETTES.iter() {
        for mode in [Mode::Light, Mode::Dark] {
            assert_eq!(
                matrix(*palette, mode).len(),
                PER_THEME,
                "{palette:?} {mode:?} pairs"
            );
        }
    }
}

/// The tightest margins reproduce the ratios the owner measured, so the gate is not merely loose.
#[test]
fn the_tightest_margins_match_the_measured_audit() {
    let m = PALETTES[0].1.light;
    let tones = change_tokens(PaletteId::Violet).tones(Mode::Light);
    let chg = tones[ChangeKind::Chg as usize];
    let cfl = tones[ChangeKind::Cfl as usize];
    let del = tones[ChangeKind::Del as usize];
    let near = |got: f32, want: f32| (got - want).abs() < 0.01;
    assert!(
        near(contrast_ratio(chg.fg, chg.bg_strong), 4.50),
        "chg on its word fill"
    );
    assert!(
        near(contrast_ratio(cfl.fg, cfl.bg_strong), 4.50),
        "cfl on its word fill"
    );
    assert!(
        near(contrast_ratio(del.fg, del.bg_strong), 4.52),
        "del on its word fill"
    );
    assert!(
        near(contrast_ratio(m.faint, m.surface2), 4.51),
        "faint on bg2"
    );
}

/// A word fill is always stronger than its line wash, in every palette and mode.
#[test]
fn every_word_fill_is_stronger_than_its_line() {
    for (palette, _) in PALETTES.iter() {
        for mode in [Mode::Light, Mode::Dark] {
            for (at, tone) in change_tokens(*palette).tones(mode).iter().enumerate() {
                let line = contrast_ratio(tone.fg, tone.bg);
                let word = contrast_ratio(tone.fg, tone.bg_strong);
                assert!(
                    word < line,
                    "{palette:?} {mode:?} {} word {word:.2} >= line {line:.2}",
                    CHANGE_NAMES[at]
                );
            }
        }
    }
}

/// The four standard palettes share one change language; high contrast strengthens it for AAA.
#[test]
fn the_change_language_is_shared_and_high_contrast_is_stronger() {
    let standard = change_tokens(PaletteId::Violet);
    for palette in [PaletteId::Graphite, PaletteId::Ocean, PaletteId::Ember] {
        let shared = change_tokens(palette);
        assert_eq!(
            shared.light, standard.light,
            "{palette:?} light change tones"
        );
        assert_eq!(shared.dark, standard.dark, "{palette:?} dark change tones");
    }
    let strong = change_tokens(PaletteId::Contrast);
    assert_ne!(
        strong.light, standard.light,
        "high contrast must strengthen the tones"
    );
    for mode in [Mode::Light, Mode::Dark] {
        for kind in [
            ChangeKind::Add,
            ChangeKind::Del,
            ChangeKind::Chg,
            ChangeKind::Same,
        ] {
            let plain = standard.tone(kind, mode).fg;
            let bold = strong.tone(kind, mode).fg;
            assert_ne!(plain, bold, "{mode:?} {kind:?} keeps the standard tone");
        }
    }
}

/// Syntax follows the palette: high contrast moves every role away from the standard ramp.
#[test]
fn syntax_roles_follow_the_palette() {
    let standard = syntax_tokens(PaletteId::Violet);
    for palette in [PaletteId::Graphite, PaletteId::Ocean, PaletteId::Ember] {
        assert_eq!(
            syntax_tokens(palette).light,
            standard.light,
            "{palette:?} light syntax"
        );
    }
    let strong = syntax_tokens(PaletteId::Contrast);
    for mode in [Mode::Light, Mode::Dark] {
        let (plain, bold) = (standard.roles(mode), strong.roles(mode));
        for at in 0..plain.len() {
            assert_ne!(
                plain[at], bold[at],
                "{mode:?} {} keeps the standard role",
                SYNTAX_NAMES[at]
            );
            let on_card = if mode.is_dark() {
                PALETTES[4].1.dark.card
            } else {
                PALETTES[4].1.light.card
            };
            assert!(
                contrast_ratio(bold[at], on_card) >= AAA,
                "{mode:?} {} misses AAA",
                SYNTAX_NAMES[at]
            );
        }
    }
}

/// Both tables are keyed in `PaletteId` order, so the lookups can index by discriminant.
#[test]
fn the_tables_are_in_palette_id_order() {
    for (at, (id, _)) in CHANGE_TOKENS.iter().enumerate() {
        assert_eq!(
            *id as usize, at,
            "CHANGE_TOKENS {id:?} is not at its discriminant index"
        );
    }
    for (at, (id, _)) in SYNTAX_TOKENS.iter().enumerate() {
        assert_eq!(
            *id as usize, at,
            "SYNTAX_TOKENS {id:?} is not at its discriminant index"
        );
    }
}

/// High contrast reads darker text on lighter fills in light mode, and the reverse in dark.
#[test]
fn high_contrast_separates_text_from_its_fills() {
    let standard = change_tokens(PaletteId::Violet);
    let strong = change_tokens(PaletteId::Contrast);
    for kind in [ChangeKind::Add, ChangeKind::Chg, ChangeKind::Cfl] {
        let light = strong.tone(kind, Mode::Light).fg;
        let plain = standard.tone(kind, Mode::Light).fg;
        assert!(
            relative_luminance(light) < relative_luminance(plain),
            "{kind:?} light text"
        );
        let dark = strong.tone(kind, Mode::Dark).fg;
        let plain = standard.tone(kind, Mode::Dark).fg;
        assert!(
            relative_luminance(dark) > relative_luminance(plain),
            "{kind:?} dark text"
        );
    }
}
