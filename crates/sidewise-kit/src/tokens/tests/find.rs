use super::every_theme;
use crate::tokens::contrast_ratio;

#[test]
fn find_hit_text_stays_readable_on_a_hit_in_every_theme() {
    for (label, theme) in every_theme() {
        let find = theme.find;
        let ratio = contrast_ratio(find.hit_text, find.hit);
        assert!(ratio >= 4.5, "{label}: hit text on a hit is {ratio:.2}:1");
        assert_ne!(
            find.hit, theme.colors.surface,
            "{label}: a hit blends into the surface"
        );
        assert_eq!(find.hit_text, theme.colors.text, "{label}: hit text");
    }
}
