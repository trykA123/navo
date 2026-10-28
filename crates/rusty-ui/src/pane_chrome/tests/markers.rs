use gpui_kit::{BorderStyle, Styled};

use super::super::card::dashed_outline;
use super::super::{PaneMarks, marked_header, on_accent};
use crate::tokens::{ColorTheme, Mode, Theme, ThemeLook, contrast_ratio};

fn themes() -> impl Iterator<Item = Theme> {
    ColorTheme::ALL.into_iter().flat_map(|color_theme| {
        [Mode::Light, Mode::Dark].map(|mode| {
            Theme::with_look(
                mode,
                ThemeLook {
                    color_theme,
                    ..ThemeLook::default()
                },
            )
        })
    })
}

#[test]
fn numbers_and_fill_need_three_shown_panes_and_the_outline_needs_two() {
    assert_eq!(PaneMarks::new(1, 1, true, false), PaneMarks::default());
    assert_eq!(
        PaneMarks::new(2, 2, false, true),
        PaneMarks {
            number: None,
            is_filled: false,
            is_target: true,
        }
    );
    assert!(!PaneMarks::new(2, 1, true, false).is_filled);
    assert!(!PaneMarks::new(2, 2, false, true).has_target_tag());
    let active = PaneMarks::new(3, 1, true, false);
    assert_eq!((active.number, active.is_filled), (Some(1), true));
    let target = PaneMarks::new(4, 3, false, true);
    assert_eq!(target.number, Some(3));
    assert!(target.has_target_tag() && !target.is_filled);
}

#[test]
fn only_the_active_header_fills_with_the_accent() {
    for t in themes() {
        let mut filled = marked_header(&t, 1, PaneMarks::new(3, 1, true, false));
        assert_eq!(filled.style().background, Some(t.colors.accent.into()));
        let mut plain = marked_header(&t, 2, PaneMarks::new(3, 2, false, true));
        assert_eq!(plain.style().background, None);
    }
}

#[test]
fn text_on_the_accent_fill_passes_4_5_to_1() {
    for t in themes() {
        let bar = on_accent(&t);
        for ink in [bar.colors.text, bar.colors.muted] {
            let ratio = contrast_ratio(ink, t.colors.accent);
            assert!(ratio >= 4.5, "{:?} {:?}: {ratio}", t.color_theme, t.mode);
        }
        let tag = contrast_ratio(t.colors.accent_text, t.colors.background);
        assert!(tag >= 4.5, "{:?} {:?}: {tag}", t.color_theme, t.mode);
    }
}

#[test]
fn the_target_outline_is_dashed_in_the_strong_border() {
    for t in themes() {
        let mut outline = dashed_outline(&t);
        assert_eq!(outline.style().border_style, Some(BorderStyle::Dashed));
        assert_eq!(outline.style().border_color, Some(t.colors.faint));
    }
}
