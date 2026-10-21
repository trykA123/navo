use super::{look, palette};
use crate::tokens::Mode;

fn gap(a: f32, b: f32) -> f32 {
    let d = (a - b).abs();
    d.min(360. - d)
}

#[test]
fn hues_are_distinct_for_one_to_twenty_four_groups() {
    for count in 1..=24 {
        let hues: Vec<f32> = palette(count, Mode::Light).iter().map(|c| c.hue).collect();
        for (at, a) in hues.iter().enumerate() {
            for b in &hues[at + 1..] {
                assert!(gap(*a, *b) > 4., "{count} groups: {a} and {b}");
            }
        }
    }
}

#[test]
fn the_same_count_gives_the_same_colours() {
    for mode in [Mode::Light, Mode::Dark] {
        let first = palette(9, mode);
        let again = palette(9, mode);
        assert_eq!(first.len(), 9);
        for (a, b) in first.iter().zip(&again) {
            assert_eq!(a.base(), b.base());
            assert_eq!(a.part(3), b.part(3));
        }
    }
}

#[test]
fn part_shades_stay_in_a_narrow_band_and_neighbours_differ() {
    for mode in [Mode::Light, Mode::Dark] {
        let base = look(mode);
        let group = palette(3, mode)[1];
        for rank in 0..40 {
            let lightness = group.lightness(rank);
            assert!((lightness - base.lightness).abs() <= base.amplitude + 1e-6);
        }
        assert_ne!(group.part(0).bg, group.part(1).bg);
        assert_ne!(group.part(1).bg, group.part(2).bg);
        assert_ne!(group.part(0).bg, group.base().bg);
    }
}
