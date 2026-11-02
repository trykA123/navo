use gpui_kit::component::theme::oklch;
use gpui_kit::{Hsla, px};

use super::{Radii, Spacing, Typography};

pub(super) fn ok(lightness_pct: f32, chroma: f32, hue: f32) -> Hsla {
    oklch(lightness_pct / 100.0, chroma, hue)
}

pub(super) fn typography() -> Typography {
    Typography {
        ui_font: "Inter".into(),
        brand_font: "Space Grotesk".into(),
        numbers_font: "Inter".into(),
        numbers_font_features: gpui_kit::FontFeatures(std::sync::Arc::new(vec![(
            "tnum".into(),
            1,
        )])),
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
        pinned_tab_label: px(96.),
        path_height: px(28.),
        header_height: px(46.),
        status_height: px(30.),
        fkey_height: px(34.),
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
        full: px(999.),
        keycap: px(6.),
        sheet: px(12.),
        job_card: px(12.),
    }
}
