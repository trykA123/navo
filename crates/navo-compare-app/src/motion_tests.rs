//! Tests of the motion gate (NC68): the tokens reach every helper, reduced motion stops all
//! animation, and no transition is short enough or bouncy enough to flash.

use std::time::Duration;

use gpui_kit::{ReadGlobal, TestAppContext};
use navo_compare_profile::appearance::PaletteId;
use sidewise_kit::tokens::Mode;

use crate::motion::{anim, ease_out_quint, hover, menu, row, screen, sheet, side, theme};
use crate::theme::{SidewiseTheme, apply_sidewise};
use crate::theme_data::palettes::MOTION;

/// A test app with the Sidewise tokens registered and the given reduced-motion flag.
fn motion_app(cx: &mut TestAppContext, reduce: bool) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        apply_sidewise(PaletteId::Violet, Mode::Light, cx);
        cx.set_reduce_motion(reduce);
    });
}

/// `anim` reads the Sidewise token while motion is allowed, and returns `None` once the flag is
/// set — including when the flag flips at runtime, so the gate is read live and never cached.
#[gpui_kit::test]
fn anim_follows_the_token_and_the_reduced_motion_flag(cx: &mut TestAppContext) {
    motion_app(cx, false);
    cx.update(|cx| {
        let animation = anim("sidebar", MOTION.side_ms, cx).expect("motion is allowed");
        assert_eq!(
            animation.duration,
            Duration::from_millis(240),
            "the side token is 240ms"
        );
        assert_eq!(
            animation.duration,
            side(cx),
            "anim agrees with the side helper"
        );
    });
    cx.update(|cx| cx.set_reduce_motion(true));
    cx.update(|cx| {
        assert!(cx.reduce_motion(), "the flag must be set");
        assert!(
            anim("sidebar", MOTION.side_ms, cx).is_none(),
            "reduced motion must stop the animation"
        );
        assert!(
            anim("toast", MOTION.menu_ms, cx).is_none(),
            "reduced motion must stop every duration"
        );
    });
}

/// The seven helpers read the seven `--dur-*` tokens of the active Sidewise theme.
#[gpui_kit::test]
fn the_seven_helpers_read_the_sidewise_motion_tokens(cx: &mut TestAppContext) {
    motion_app(cx, false);
    cx.update(|cx| {
        let m = SidewiseTheme::global(cx).motion;
        assert_eq!(hover(cx), Duration::from_millis(m.hover_ms), "hover");
        assert_eq!(menu(cx), Duration::from_millis(m.menu_ms), "menu");
        assert_eq!(theme(cx), Duration::from_millis(m.theme_ms), "theme");
        assert_eq!(screen(cx), Duration::from_millis(m.screen_ms), "screen");
        assert_eq!(row(cx), Duration::from_millis(m.row_ms), "row");
        assert_eq!(side(cx), Duration::from_millis(m.side_ms), "side");
        assert_eq!(sheet(cx), Duration::from_millis(m.sheet_ms), "sheet");
    });
}

/// Photosensitivity: every transition sits in the prototype's 150-300 ms band, so none is a
/// fast full-screen change and none is short enough to read as a flash.
#[gpui_kit::test]
fn every_transition_is_inside_the_prototype_motion_band(cx: &mut TestAppContext) {
    motion_app(cx, false);
    cx.update(|cx| {
        let all = [
            ("hover", hover(cx)),
            ("menu", menu(cx)),
            ("theme", theme(cx)),
            ("screen", screen(cx)),
            ("row", row(cx)),
            ("side", side(cx)),
            ("sheet", sheet(cx)),
        ];
        for (name, duration) in all {
            let ms = duration.as_millis();
            assert!(
                (150..=300).contains(&ms),
                "{name} is {ms}ms, outside the 150-300ms band"
            );
        }
    });
}

/// Nothing loops and nothing overshoots: a repeating or bouncing animation could flash.
#[gpui_kit::test]
fn animations_are_one_shot_and_the_easing_never_overshoots(cx: &mut TestAppContext) {
    motion_app(cx, false);
    cx.update(|cx| {
        let animation = anim("sheet", MOTION.sheet_ms, cx).expect("motion is allowed");
        assert!(animation.oneshot, "a looping animation could flash");
        assert!(!animation.synced, "no shared repeating clock");
    });
    for at in 0..=100u8 {
        let t = f32::from(at) / 100.0;
        let value = ease_out_quint(t);
        assert!(
            (0.0..=1.0).contains(&value),
            "ease({t}) = {value} leaves 0..1"
        );
    }
    assert_eq!(ease_out_quint(-1.0), 0.0, "below range clamps to the start");
    assert_eq!(ease_out_quint(2.0), 1.0, "above range clamps to the end");
}

/// The easing is the prototype's `--ease` curve, measured against independently solved values.
#[test]
fn ease_out_quint_matches_the_prototype_curve() {
    let published = [
        (0.0, 0.0),
        (0.1, 0.396_694),
        (0.25, 0.767_284),
        (0.5, 0.946_079),
        (0.75, 0.991_108),
        (0.9, 0.998_830),
        (1.0, 1.0),
    ];
    for (t, want) in published {
        let got = ease_out_quint(t);
        assert!((got - want).abs() < 1e-4, "ease({t}) = {got}, want {want}");
    }
}
