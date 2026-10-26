use rusty_core::shell_menu::Icon;

const MAX_ICON_SIDE: i32 = 256;

pub(super) fn icon_size(width: i32, height: i32) -> Option<(u32, u32)> {
    let side = |value: i32| {
        (1..=MAX_ICON_SIDE)
            .contains(&value)
            .then(|| value.unsigned_abs())
    };
    Some((side(width)?, side(height)?))
}

pub(super) fn icon_from_bgra(width: u32, height: u32, mut data: Vec<u8>) -> Option<Icon> {
    let expected = usize::try_from(u64::from(width) * u64::from(height) * 4).ok()?;
    if width == 0 || height == 0 || data.len() != expected {
        return None;
    }
    let has_alpha = data.as_chunks::<4>().0.iter().any(|px| px[3] != 0);
    let premultiplied = has_alpha
        && data
            .as_chunks::<4>()
            .0
            .iter()
            .all(|px| px[..3].iter().all(|channel| *channel <= px[3]));
    for px in data.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
        if !has_alpha {
            px[3] = u8::MAX;
        } else if premultiplied {
            let alpha = px[3];
            px[..3]
                .iter_mut()
                .for_each(|channel| *channel = unpremultiply(*channel, alpha));
        }
    }
    Some(Icon {
        width,
        height,
        rgba: data,
    })
}

/// An owner-drawn icon, drawn once on black and once on white: the difference gives its alpha.
pub(super) fn icon_from_two(
    width: u32,
    height: u32,
    on_black: &[u8],
    on_white: &[u8],
) -> Option<Icon> {
    let expected = usize::try_from(u64::from(width) * u64::from(height) * 4).ok()?;
    if width == 0 || height == 0 || on_black.len() != expected || on_white.len() != expected {
        return None;
    }
    let mut rgba = Vec::with_capacity(expected);
    for (black, white) in on_black
        .as_chunks::<4>()
        .0
        .iter()
        .zip(on_white.as_chunks::<4>().0)
    {
        let spread: u16 = (0..3)
            .map(|at| u16::from(white[at].saturating_sub(black[at])))
            .sum();
        let alpha = u8::try_from(255 - (spread / 3).min(255)).unwrap_or(0);
        let [blue, green, red] = [black[0], black[1], black[2]].map(|c| unpremultiply(c, alpha));
        rgba.extend_from_slice(&[red, green, blue, alpha]);
    }
    rgba.as_chunks::<4>()
        .0
        .iter()
        .any(|px| px[3] != 0)
        .then_some(Icon {
            width,
            height,
            rgba,
        })
}

fn unpremultiply(channel: u8, alpha: u8) -> u8 {
    if alpha == 0 {
        return 0;
    }
    let straight = u16::from(channel) * 255 / u16::from(alpha);
    u8::try_from(straight).unwrap_or(u8::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blue_green_red_alpha_becomes_red_green_blue_alpha() {
        let icon = icon_from_bgra(1, 1, vec![10, 20, 200, 255]);
        assert_eq!(icon.map(|i| i.rgba), Some(vec![200, 20, 10, 255]));
    }

    #[test]
    fn a_bitmap_without_alpha_is_opaque() {
        let icon = icon_from_bgra(2, 1, vec![1, 2, 3, 0, 4, 5, 6, 0]);
        assert_eq!(icon.map(|i| i.rgba), Some(vec![3, 2, 1, 255, 6, 5, 4, 255]));
    }

    #[test]
    fn premultiplied_pixels_are_made_straight() {
        let icon = icon_from_bgra(2, 1, vec![0, 0, 64, 128, 0, 0, 0, 0]);
        assert_eq!(icon.map(|i| i.rgba), Some(vec![127, 0, 0, 128, 0, 0, 0, 0]));
    }

    #[test]
    fn straight_alpha_is_left_alone() {
        let icon = icon_from_bgra(1, 1, vec![0, 0, 200, 128]);
        assert_eq!(icon.map(|i| i.rgba), Some(vec![200, 0, 0, 128]));
    }

    #[test]
    fn menu_sized_bitmaps_are_accepted() {
        assert_eq!(icon_size(16, 16), Some((16, 16)));
        assert_eq!(icon_size(256, 256), Some((256, 256)));
    }

    #[test]
    fn bitmaps_over_256_pixels_are_skipped() {
        assert_eq!(icon_size(257, 16), None);
        assert_eq!(icon_size(16, 4096), None);
    }

    #[test]
    fn zero_or_negative_sizes_are_skipped() {
        assert_eq!(icon_size(0, 16), None);
        assert_eq!(icon_size(16, -16), None);
    }

    #[test]
    fn an_opaque_drawing_keeps_its_colour() {
        let pixel = [10, 20, 200, 0];
        let icon = icon_from_two(1, 1, &pixel, &pixel);
        assert_eq!(icon.map(|i| i.rgba), Some(vec![200, 20, 10, 255]));
    }

    #[test]
    fn the_background_shows_through_as_transparency() {
        let on_black = [0, 0, 0, 0, 0, 0, 128, 0];
        let on_white = [255, 255, 255, 0, 127, 127, 255, 0];
        let icon = icon_from_two(2, 1, &on_black, &on_white).map(|i| i.rgba);
        assert_eq!(icon, Some(vec![0, 0, 0, 0, 255, 0, 0, 128]));
    }

    #[test]
    fn nothing_drawn_gives_no_icon() {
        let on_black = vec![0; 16];
        let on_white = vec![255; 16];
        assert_eq!(icon_from_two(2, 2, &on_black, &on_white), None);
    }

    #[test]
    fn two_drawings_of_different_sizes_give_no_icon() {
        assert_eq!(icon_from_two(1, 1, &[0; 4], &[0; 8]), None);
    }

    #[test]
    fn a_size_mismatch_gives_no_icon() {
        assert_eq!(icon_from_bgra(2, 2, vec![0; 4]), None);
        assert_eq!(icon_from_bgra(0, 0, Vec::new()), None);
    }
}
