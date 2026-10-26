use gpui_kit::WindowAppearance;
use gpui_kit::component::theme::Theme as KitTheme;

use super::FerriteWindow;
use crate::prelude::*;
use crate::settings::{Field, Value, Values};
use crate::status_bar::StatusNotice;
use crate::tokens::{self, Mode, Typography};

const MIN_FONT_SIZE: i64 = 10;
const MAX_FONT_SIZE: i64 = 20;

impl FerriteWindow {
    pub(super) fn apply_theme(&mut self, window: &Window, cx: &mut App) {
        let mode = self.values.theme().resolve(mode_of(window.appearance()));
        let look = tokens::ThemeLook {
            density: self.values.density(),
            syntax: self.values.syntax_theme(),
            color_theme: self.values.color_theme(),
        };
        tokens::apply_look(mode, look, cx);
        let families = cx.text_system().all_font_names();
        let mut theme = tokens::current(cx).clone();
        let base = theme.typography.body;
        self.saver.font_notice = apply_typography(&self.values, &families, &mut theme.typography);
        let ratio = theme.typography.body / base;
        scale_heights(&mut theme, ratio);
        let typography = theme.typography.clone();
        *cx.global_mut::<tokens::Theme>() = theme;
        KitTheme::update(cx, |kit| {
            kit.font_family = typography.ui_font;
            kit.font_size = typography.body;
        });
    }
}

fn scale_heights(theme: &mut tokens::Theme, ratio: f32) {
    let scaled = |height: Pixels| px((f32::from(height) * ratio).round());
    let spacing = &mut theme.spacing;
    spacing.row_height = scaled(spacing.row_height);
    spacing.tab_height = scaled(spacing.tab_height);
    spacing.path_height = scaled(spacing.path_height);
    theme.rows.height = scaled(theme.rows.height);
}

fn apply_typography(
    values: &Values,
    families: &[String],
    typography: &mut Typography,
) -> Option<StatusNotice> {
    let mut notices = Vec::new();
    if let Value::Text(family) = values.get(Field::Font) {
        let family = family.trim();
        if !family.is_empty() && !family.eq_ignore_ascii_case(typography.ui_font.as_ref()) {
            match families
                .iter()
                .find(|name| name.eq_ignore_ascii_case(family))
            {
                Some(name) => {
                    typography.ui_font = name.clone().into();
                    typography.numbers_font = name.clone().into();
                }
                None => notices.push(rusty_l10n::t!("settings-font-unavailable", family = family)),
            }
        }
    }
    if let Value::Number(size) = values.get(Field::FontSize) {
        let clamped = (*size).clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
        if *size != clamped {
            notices.push(rusty_l10n::t!("settings-font-size-clamped", size = clamped));
        }
        let ratio = px(clamped as f32) / typography.body;
        typography.body = px(clamped as f32);
        typography.small *= ratio;
        typography.tiny *= ratio;
        typography.key *= ratio;
        typography.line_height *= ratio;
    }
    (!notices.is_empty()).then(|| StatusNotice {
        text: notices.join(" · ").into(),
        is_error: false,
    })
}

pub(super) fn mode_of(appearance: WindowAppearance) -> Mode {
    match appearance {
        WindowAppearance::Dark | WindowAppearance::VibrantDark => Mode::Dark,
        WindowAppearance::Light | WindowAppearance::VibrantLight => Mode::Light,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Change;

    #[test]
    fn family_changes_keep_brand_mono_and_tabular_features() {
        let mut typography = tokens::Theme::new(tokens::Mode::Light).typography;
        let defaults = typography.clone();
        let values = Values::default().with(&Change::text(Field::Font, "Installed Sans"));
        let notice = apply_typography(&values, &["Installed Sans".to_owned()], &mut typography);
        assert!(notice.is_none());
        assert_eq!(typography.ui_font.as_ref(), "Installed Sans");
        assert_eq!(typography.numbers_font, typography.ui_font);
        assert_eq!(typography.brand_font, defaults.brand_font);
        assert_eq!(typography.mono_font, defaults.mono_font);
        assert_eq!(
            typography.numbers_font_features,
            defaults.numbers_font_features
        );
    }

    #[test]
    fn empty_family_uses_the_default_without_a_notice() {
        let mut typography = tokens::Theme::new(tokens::Mode::Light).typography;
        let defaults = typography.clone();
        let values = Values::default().with(&Change::text(Field::Font, ""));
        assert!(apply_typography(&values, &[], &mut typography).is_none());
        assert_eq!(typography, defaults);
    }

    fn sized(size: i64) -> tokens::Theme {
        let mut theme = tokens::Theme::new(tokens::Mode::Light);
        let base = theme.typography.body;
        let values = Values::default().with(&Change {
            field: Field::FontSize,
            value: Value::Number(size),
        });
        apply_typography(&values, &[], &mut theme.typography);
        let ratio = theme.typography.body / base;
        scale_heights(&mut theme, ratio);
        theme
    }

    #[test]
    fn the_default_font_size_keeps_every_height() {
        let defaults = tokens::Theme::new(tokens::Mode::Light);
        let theme = sized(13);
        assert_eq!(theme.spacing, defaults.spacing);
        assert_eq!(theme.rows, defaults.rows);
    }

    #[test]
    fn rows_tabs_and_the_path_bar_grow_with_the_largest_font() {
        let theme = sized(MAX_FONT_SIZE);
        let line = theme.typography.line_height;
        assert!(
            theme.rows.height >= line,
            "row {:?} < line {line:?}",
            theme.rows.height
        );
        assert!(theme.spacing.tab_height >= line);
        assert!(theme.spacing.path_height >= line);
    }

    #[test]
    fn out_of_range_sizes_are_clamped_with_a_notice() {
        for (requested, expected) in [(0, MIN_FONT_SIZE), (99, MAX_FONT_SIZE)] {
            let mut typography = tokens::Theme::new(tokens::Mode::Light).typography;
            let values = Values::default().with(&Change {
                field: Field::FontSize,
                value: Value::Number(requested),
            });
            let notice = apply_typography(&values, &[], &mut typography);
            assert_eq!(typography.body, px(expected as f32));
            assert!(notice.is_some_and(|notice| !notice.is_error && !notice.text.is_empty()));
        }
    }
}
