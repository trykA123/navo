use std::sync::OnceLock;

use gpui_kit::Hsla;

use super::Theme;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SystemPalette {
    pub window: Hsla,
    pub window_text: Hsla,
    pub highlight: Hsla,
    pub highlight_text: Hsla,
    pub button_face: Hsla,
    pub button_text: Hsla,
    pub gray_text: Hsla,
    pub hot_light: Hsla,
}

pub fn contrast_theme(base: &Theme, p: &SystemPalette) -> Theme {
    let mut theme = base.clone();
    theme.is_contrast = true;
    theme.colors = contrast_colors(p);
    theme.surfaces = contrast_surfaces(p);
    theme.shadows = contrast_shadows(p);
    theme.marks = super::MarkColors {
        fills: [p.window; 8],
        ticks: [p.window_text; 8],
    };
    let kind = super::MemKind {
        strong: p.hot_light,
        tint: p.window,
        text: p.window_text,
    };
    theme.mem = super::MemColors {
        code: kind,
        rodata: kind,
        data: kind,
        bss: kind,
        other: kind,
        fill: kind,
        free: p.window,
        free_line: p.window_text,
    };
    let tone = super::DiffTone {
        text: p.window_text,
        mark: p.hot_light,
        fill: p.window,
        wash: p.window,
    };
    theme.diff = super::DiffColors {
        delete: tone,
        add: tone,
        change: tone,
        moved: tone,
        conflict: tone,
        auto_merged: tone,
        gap_hatch: p.window_text,
    };
    theme
}

fn contrast_colors(p: &SystemPalette) -> super::Colors {
    super::Colors {
        background: p.window,
        surface: p.window,
        rail: p.window,
        zebra: p.window,
        line: p.window_text,
        text: p.window_text,
        muted: p.gray_text,
        faint: p.gray_text,
        hover: p.window,
        selection: p.highlight,
        on_selection: p.highlight_text,
        on_button: p.button_text,
        accent: p.hot_light,
        accent_text: p.hot_light,
        mark: p.window_text,
        folder: p.window_text,
        kind_code: p.window_text,
        kind_image: p.window_text,
        kind_archive: p.window_text,
        kind_app: p.window_text,
        kind_pdf: p.window_text,
        kind_doc: p.window_text,
        kind_sheet: p.window_text,
        kind_slides: p.window_text,
        kind_audio: p.window_text,
        kind_video: p.window_text,
        kind_auto: p.window_text,
        ok: p.window_text,
        warn: p.window_text,
        terminal_background: p.window,
        terminal_text: p.window_text,
        terminal_muted: p.gray_text,
    }
}

fn contrast_surfaces(p: &SystemPalette) -> super::Surfaces {
    super::Surfaces {
        mica_from: p.window,
        mica_to: p.window,
        card: p.window,
        card_edge: p.window_text,
        card_ring: p.hot_light,
        current: p.highlight,
        field: p.window,
        keycap_face: p.button_face,
        keycap_edge: p.button_text,
        keycap_label: p.button_text,
        keycap_lit: p.button_face,
        keycap_lit_label: p.button_text,
        layer_tint: p.window,
        pill: p.button_face,
        pill_ring: p.hot_light,
        pill_label: p.button_text,
        progress_track: p.window,
        progress_fill: p.highlight,
        job_card: p.window,
        sheet: p.window,
        scrim: p.window,
        tile: p.window,
        tile_edge: p.window_text,
    }
}

fn contrast_shadows(p: &SystemPalette) -> super::Shadows {
    super::Shadows {
        ink: p.window,
        card: vec![],
        card_active: vec![],
        job_card: vec![],
        pill: vec![],
        keycap: vec![],
        menu: vec![],
        chip: vec![],
        sheet: vec![],
        sheet_ring: vec![],
        up: vec![],
        focus: vec![gpui_kit::BoxShadow {
            color: p.hot_light,
            offset: gpui_kit::point(gpui_kit::px(0.), gpui_kit::px(0.)),
            blur_radius: gpui_kit::px(0.),
            spread_radius: gpui_kit::px(2.),
            inset: false,
        }],
    }
}

impl Theme {
    pub(crate) fn selected_content(&self, is_selected: bool) -> std::borrow::Cow<'_, Self> {
        if !is_selected || !self.is_contrast {
            return std::borrow::Cow::Borrowed(self);
        }
        let mut theme = self.clone();
        let c = &mut theme.colors;
        let color = c.on_selection;
        *c = super::Colors {
            text: color,
            muted: color,
            faint: color,
            accent: color,
            accent_text: color,
            mark: color,
            folder: color,
            kind_code: color,
            kind_image: color,
            kind_archive: color,
            kind_app: color,
            kind_pdf: color,
            kind_doc: color,
            kind_sheet: color,
            kind_slides: color,
            kind_audio: color,
            kind_video: color,
            kind_auto: color,
            rail: c.selection,
            ..*c
        };
        theme.surfaces.tile = c.selection;
        theme.surfaces.tile_edge = color;
        theme.surfaces.card_ring = color;
        std::borrow::Cow::Owned(theme)
    }
}

pub(crate) fn with_system_palette(
    base: &Theme,
    read: impl FnOnce() -> Option<SystemPalette>,
) -> Theme {
    read().map_or_else(|| base.clone(), |palette| contrast_theme(base, &palette))
}

pub(crate) fn map_contrast_colors(theme: &Theme, k: &mut gpui_kit::component::theme::ThemeColor) {
    if !theme.is_contrast {
        return;
    }
    let c = &theme.colors;
    k.background = c.background;
    k.foreground = c.text;
    k.border = c.line;
    k.input = c.line;
    k.ring = c.accent;
    k.muted = c.background;
    k.muted_foreground = c.muted;
    k.accent = c.selection;
    k.accent_foreground = c.on_selection;
    k.primary = theme.surfaces.keycap_face;
    k.primary_hover = k.primary;
    k.primary_active = k.primary;
    k.primary_foreground = c.on_button;
    k.button_primary = k.primary;
    k.button_primary_hover = k.primary;
    k.button_primary_active = k.primary;
    k.button_primary_foreground = c.on_button;
    k.danger = k.primary;
    k.danger_hover = k.primary;
    k.danger_active = k.primary;
    k.danger_foreground = c.on_button;
    k.button_danger = k.primary;
    k.button_danger_hover = k.primary;
    k.button_danger_active = k.primary;
    k.button_danger_foreground = c.on_button;
    k.button = k.primary;
    k.button_hover = k.primary;
    k.button_active = k.primary;
    k.button_foreground = c.on_button;
    k.secondary = k.primary;
    k.secondary_hover = k.primary;
    k.secondary_active = k.primary;
    k.secondary_foreground = c.on_button;
    k.progress_bar = c.selection;
    k.scrollbar = c.background;
    k.scrollbar_thumb = c.text;
    k.scrollbar_thumb_hover = c.text;
    k.popover = c.background;
    k.popover_foreground = c.text;
    k.tab_bar = c.background;
    k.tab = c.background;
    k.tab_active = c.background;
    k.tab_foreground = c.muted;
    k.tab_active_foreground = c.text;
}

pub fn from_colorref(value: u32) -> Hsla {
    gpui_kit::rgb(((value & 0xff) << 16) | (value & 0xff00) | ((value >> 16) & 0xff)).into()
}

static READER: OnceLock<fn() -> Option<SystemPalette>> = OnceLock::new();

pub fn set_system_palette_reader(read: fn() -> Option<SystemPalette>) {
    let _ = READER.set(read);
}

pub fn system_palette() -> Option<SystemPalette> {
    READER.get().and_then(|read| read())
}

#[cfg(test)]
mod tests {
    use gpui_kit::{black, px, rgb, white};

    use super::*;
    use crate::tokens::{ColorTheme, Mode, ThemeLook, contrast_ratio};

    fn palette(is_dark: bool) -> SystemPalette {
        let (window, window_text) = if is_dark {
            (black(), white())
        } else {
            (white(), black())
        };
        SystemPalette {
            window,
            window_text,
            highlight: window_text,
            highlight_text: window,
            button_face: window_text,
            button_text: window,
            gray_text: rgb(0x767676).into(),
            hot_light: window_text,
        }
    }

    #[test]
    fn a_colorref_is_read_as_blue_green_red() {
        assert_eq!(from_colorref(0x0000_00ff), rgb(0xff_0000).into());
        assert_eq!(from_colorref(0x00ff_0000), rgb(0x00_00ff).into());
    }

    #[test]
    fn built_in_content_tokens_keep_the_existing_foregrounds() {
        for color_theme in ColorTheme::ALL {
            for mode in [Mode::Light, Mode::Dark] {
                let base = Theme::with_look(
                    mode,
                    ThemeLook {
                        color_theme,
                        ..ThemeLook::default()
                    },
                );
                assert_eq!(base.colors.on_selection, base.colors.text);
                assert_eq!(base.colors.on_button, base.colors.text);
            }
        }
    }

    #[test]
    fn system_pairs_preserve_seven_to_one_contrast_on_black_and_white() {
        for is_dark in [true, false] {
            let palette = palette(is_dark);
            let base = Theme::new(Mode::Light);
            let theme = contrast_theme(&base, &palette);
            let c = theme.colors;
            for (foreground, background) in [
                (c.text, c.background),
                (c.on_selection, c.selection),
                (c.on_button, theme.surfaces.keycap_face),
            ] {
                assert!(contrast_ratio(foreground, background) >= 7.);
            }
            assert_eq!(c.background, palette.window);
            assert_eq!(c.text, palette.window_text);
            assert_eq!(c.selection, palette.highlight);
            assert_eq!(c.on_selection, palette.highlight_text);
            assert_eq!(c.on_button, palette.button_text);
            assert_eq!(c.muted, palette.gray_text);
            assert_eq!(c.accent, palette.hot_light);
            assert!(theme.is_contrast);
            assert_eq!(base, Theme::new(Mode::Light));
        }
    }

    #[test]
    fn injected_none_preserves_the_entire_theme() {
        let mut base = Theme::new(Mode::Dark);
        base.scale(150);
        assert_eq!(with_system_palette(&base, || None), base);
        assert_eq!(
            with_system_palette(&base, || Some(palette(true))),
            contrast_theme(&base, &palette(true))
        );
    }

    #[test]
    fn contrast_removes_depth_and_translucency_but_keeps_the_focus_ring() {
        let theme = contrast_theme(&Theme::new(Mode::Dark), &palette(true));
        let s = &theme.shadows;
        for layers in [
            &s.card,
            &s.card_active,
            &s.job_card,
            &s.pill,
            &s.keycap,
            &s.menu,
            &s.chip,
            &s.sheet,
            &s.sheet_ring,
            &s.up,
        ] {
            assert!(layers.is_empty());
        }
        assert_eq!(s.focus.len(), 1);
        assert_eq!(s.focus[0].color, theme.colors.accent);
        assert_eq!(s.focus[0].spread_radius, px(2.));
        assert_eq!(s.focus[0].blur_radius, px(0.));
        assert_eq!(theme.surfaces.mica_from, theme.surfaces.mica_to);
        assert!(
            theme
                .colors
                .entries()
                .iter()
                .all(|(_, color)| color.a == 1.)
        );
        assert!(
            theme
                .surfaces
                .entries()
                .iter()
                .all(|(_, color)| color.a == 1.)
        );
    }
}
