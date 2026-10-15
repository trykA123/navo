use gpui_kit::{Hsla, Rgba};

fn linear(channel: f32) -> f32 {
    if channel <= 0.040_45 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

pub fn relative_luminance(color: Hsla) -> f32 {
    let rgb = Rgba::from(color);
    0.2126 * linear(rgb.r) + 0.7152 * linear(rgb.g) + 0.0722 * linear(rgb.b)
}

pub fn contrast_ratio(foreground: Hsla, background: Hsla) -> f32 {
    let a = relative_luminance(foreground);
    let b = relative_luminance(background);
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[cfg(test)]
mod tests {
    use gpui_kit::{black, rgb, white};

    use super::{contrast_ratio, relative_luminance};

    #[test]
    fn black_on_white_is_twenty_one_to_one() {
        assert!((contrast_ratio(black(), white()) - 21.0).abs() < 0.01);
    }

    #[test]
    fn a_colour_on_itself_is_one_to_one() {
        let grey = rgb(0x777777).into();
        assert!((contrast_ratio(grey, grey) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn the_ratio_does_not_depend_on_the_order() {
        let blue = rgb(0x1d4ed8).into();
        assert!((contrast_ratio(blue, white()) - contrast_ratio(white(), blue)).abs() < 1e-6);
    }

    #[test]
    fn grey_767676_on_white_is_just_above_the_aa_minimum() {
        let ratio = contrast_ratio(rgb(0x767676).into(), white());
        assert!((ratio - 4.54).abs() < 0.01, "{ratio}");
    }

    #[test]
    fn luminance_runs_from_zero_for_black_to_one_for_white() {
        assert!(relative_luminance(black()).abs() < 1e-6);
        assert!((relative_luminance(white()) - 1.0).abs() < 1e-6);
    }
}
