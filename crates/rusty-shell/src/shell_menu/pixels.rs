use rusty_core::shell_menu::Icon;

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
    fn a_size_mismatch_gives_no_icon() {
        assert_eq!(icon_from_bgra(2, 2, vec![0; 4]), None);
        assert_eq!(icon_from_bgra(0, 0, Vec::new()), None);
    }
}
