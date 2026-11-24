use super::every_theme;
use crate::tokens::{Mode, SystemPalette, Theme, contrast_ratio, contrast_theme, system_palette};

fn assert_cue(label: &str, theme: &Theme) {
    let find = theme.find;
    let band = contrast_ratio(find.hit, theme.colors.surface);
    assert!(
        band >= 3.,
        "{label}: hit band on the raised surface is {band:.2}:1"
    );
    let text = contrast_ratio(find.hit_text, find.hit);
    assert!(text >= 4.5, "{label}: hit text on the band is {text:.2}:1");
}

#[test]
fn find_hit_band_meets_three_to_one_and_hit_text_four_point_five_in_every_palette() {
    for (label, theme) in every_theme() {
        assert_cue(&label, &theme);
    }
    for (label, dark) in [("black", true), ("white", false)] {
        let palette: SystemPalette = system_palette::palette(dark);
        assert_cue(label, &contrast_theme(&Theme::new(Mode::Light), &palette));
    }
}
