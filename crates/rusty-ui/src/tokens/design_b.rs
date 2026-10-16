use gpui_kit::component::theme::oklch;
use gpui_kit::{Hsla, px};

use super::{Colors, Mode, Radii, Spacing, Typography};

pub(super) fn ok(lightness_pct: f32, chroma: f32, hue: f32) -> Hsla {
    oklch(lightness_pct / 100.0, chroma, hue)
}

fn light() -> Colors {
    Colors {
        background: ok(99.2, 0.002, 260.),
        surface: ok(99.2, 0.002, 260.),
        rail: ok(96.2, 0.005, 260.),
        zebra: ok(97.6, 0.004, 260.),
        line: ok(88., 0.008, 260.),
        text: ok(21., 0.012, 260.),
        muted: ok(46., 0.02, 260.),
        faint: ok(60., 0.016, 260.),
        hover: ok(95., 0.008, 260.),
        selection: ok(93., 0.035, 262.),
        accent: ok(50., 0.18, 262.),
        accent_text: ok(46., 0.17, 262.),
        mark: ok(53., 0.2, 30.),
        folder: ok(60., 0.13, 80.),
        kind_code: ok(50., 0.15, 262.),
        kind_image: ok(52., 0.15, 340.),
        kind_archive: ok(52., 0.12, 45.),
        kind_app: ok(50., 0.12, 150.),
        ok: ok(50., 0.13, 150.),
        warn: ok(55., 0.13, 70.),
        terminal_background: ok(97., 0.004, 260.),
        terminal_text: ok(25., 0.015, 260.),
        terminal_muted: ok(50., 0.02, 260.),
    }
}

fn dark() -> Colors {
    Colors {
        background: ok(16., 0.008, 260.),
        surface: ok(16., 0.008, 260.),
        rail: ok(19., 0.01, 260.),
        zebra: ok(18., 0.009, 260.),
        line: ok(28., 0.012, 260.),
        text: ok(93., 0.006, 260.),
        muted: ok(72., 0.016, 260.),
        faint: ok(58., 0.014, 260.),
        hover: ok(22., 0.012, 260.),
        selection: ok(30., 0.07, 262.),
        accent: ok(72., 0.13, 262.),
        accent_text: ok(78., 0.11, 262.),
        mark: ok(72., 0.16, 30.),
        folder: ok(80., 0.11, 85.),
        kind_code: ok(76., 0.1, 262.),
        kind_image: ok(75., 0.11, 340.),
        kind_archive: ok(75., 0.1, 45.),
        kind_app: ok(75., 0.1, 150.),
        ok: ok(76., 0.13, 150.),
        warn: ok(80., 0.12, 75.),
        terminal_background: ok(12., 0.008, 260.),
        terminal_text: ok(90., 0.008, 260.),
        terminal_muted: ok(66., 0.016, 260.),
    }
}

pub(super) fn colors(mode: Mode) -> Colors {
    match mode {
        Mode::Light => light(),
        Mode::Dark => dark(),
    }
}

pub(super) fn typography() -> Typography {
    Typography {
        ui_font: "Archivo".into(),
        numbers_font: "Archivo".into(),
        mono_font: "JetBrains Mono".into(),
        body: px(13.),
        small: px(12.5),
        tiny: px(11.),
        key: px(10.5),
        line_height: px(17.5),
    }
}

pub(super) fn spacing() -> Spacing {
    Spacing {
        xxs: px(2.),
        xs: px(4.),
        sm: px(6.),
        md: px(8.),
        lg: px(10.),
        xl: px(12.),
        xxl: px(16.),
        icon: px(15.),
        row_height: px(24.),
        tab_height: px(28.),
        path_height: px(28.),
        header_height: px(46.),
        status_height: px(26.),
        fkey_height: px(36.),
        rail_width: px(236.),
        rail_item_height: px(30.),
        drawer_height: px(220.),
        size_column: px(80.),
        date_column: px(96.),
    }
}

pub(super) fn radii() -> Radii {
    Radii {
        xs: px(3.),
        sm: px(4.),
        md: px(6.),
        lg: px(7.),
        card: px(8.),
        pill: px(15.),
        keycap: px(6.),
        sheet: px(12.),
    }
}
