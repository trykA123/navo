//! Colour text for the image viewer's eyedropper: one 8-bit sRGB pixel as Hex, `rgb()` or `oklch()`.
//! Pure functions with fixed rounding, so copied values are stable across runs and platforms.

/// One straight-alpha 8-bit sRGB pixel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::window_view::viewer) struct Rgba8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// The three text formats the eyedropper shows and copies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::window_view::viewer) enum ColourFormat {
    Hex,
    Rgb,
    Oklch,
}

impl ColourFormat {
    pub(in crate::window_view::viewer) const ALL: [Self; 3] = [Self::Hex, Self::Rgb, Self::Oklch];
}

/// Turns an sRGB channel (0–255) into linear light (0–1) with the sRGB transfer curve.
fn linear(channel: u8) -> f64 {
    let c = f64::from(channel) / 255.;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Converts a pixel to OKLab (L 0–1, a, b) with Björn Ottosson's published matrices.
fn oklab(pixel: Rgba8) -> (f64, f64, f64) {
    let (r, g, b) = (linear(pixel.r), linear(pixel.g), linear(pixel.b));
    let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
    let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
    let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
    (
        0.210_454_255_3 * l + 0.793_617_785 * m - 0.004_072_046_8 * s,
        1.977_998_495_1 * l - 2.428_592_205 * m + 0.450_593_709_9 * s,
        0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766 * s,
    )
}

/// Rounds `value` to `places` decimals and turns a negative zero into zero.
fn rounded(value: f64, places: i32) -> f64 {
    let scale = 10f64.powi(places);
    let value = (value * scale).round() / scale;
    if value == 0. { 0. } else { value }
}

impl Rgba8 {
    /// `#RRGGBB`, or `#RRGGBBAA` when the pixel is not fully opaque; upper-case digits.
    pub(in crate::window_view::viewer) fn hex(self) -> String {
        let Self { r, g, b, a } = self;
        if a == u8::MAX {
            format!("#{r:02X}{g:02X}{b:02X}")
        } else {
            format!("#{r:02X}{g:02X}{b:02X}{a:02X}")
        }
    }

    /// CSS `rgb(r, g, b)` with the 0–255 channels; alpha is left out.
    pub(in crate::window_view::viewer) fn rgb(self) -> String {
        format!("rgb({}, {}, {})", self.r, self.g, self.b)
    }

    /// CSS `oklch(L% C H)`: L in percent (one decimal), C three decimals, H degrees (one decimal).
    /// A grey has no hue, so H reads 0.0 when C rounds to zero.
    pub(in crate::window_view::viewer) fn oklch(self) -> String {
        let (l, a, b) = oklab(self);
        let chroma = rounded(a.hypot(b), 3);
        let mut hue = if chroma == 0. {
            0.
        } else {
            rounded(b.atan2(a).to_degrees().rem_euclid(360.), 1)
        };
        if hue >= 360. {
            hue = 0.;
        }
        format!("oklch({:.1}% {chroma:.3} {hue:.1})", rounded(l * 100., 1))
    }

    /// The pixel as text in `format`.
    pub(in crate::window_view::viewer) fn text(self, format: ColourFormat) -> String {
        match format {
            ColourFormat::Hex => self.hex(),
            ColourFormat::Rgb => self.rgb(),
            ColourFormat::Oklch => self.oklch(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Rgba8;

    fn rgba(r: u8, g: u8, b: u8, a: u8) -> Rgba8 {
        Rgba8 { r, g, b, a }
    }

    #[test]
    fn pure_red_reads_in_all_three_formats() {
        let red = rgba(255, 0, 0, 255);
        assert_eq!(red.hex(), "#FF0000");
        assert_eq!(red.rgb(), "rgb(255, 0, 0)");
        assert_eq!(red.oklch(), "oklch(62.8% 0.258 29.2)");
    }

    #[test]
    fn white_is_full_lightness_with_no_chroma_or_hue() {
        let white = rgba(255, 255, 255, 255);
        assert_eq!(white.hex(), "#FFFFFF");
        assert_eq!(white.rgb(), "rgb(255, 255, 255)");
        assert_eq!(white.oklch(), "oklch(100.0% 0.000 0.0)");
    }

    #[test]
    fn black_is_zero_everywhere() {
        let black = rgba(0, 0, 0, 255);
        assert_eq!(black.hex(), "#000000");
        assert_eq!(black.rgb(), "rgb(0, 0, 0)");
        assert_eq!(black.oklch(), "oklch(0.0% 0.000 0.0)");
    }

    #[test]
    fn mid_grey_has_no_hue_and_sixty_percent_lightness() {
        let grey = rgba(128, 128, 128, 255);
        assert_eq!(grey.hex(), "#808080");
        assert_eq!(grey.rgb(), "rgb(128, 128, 128)");
        assert_eq!(grey.oklch(), "oklch(60.0% 0.000 0.0)");
    }

    #[test]
    fn a_translucent_pixel_adds_alpha_to_hex_only() {
        let blue = rgba(30, 30, 220, 128);
        assert_eq!(blue.hex(), "#1E1EDC80");
        assert_eq!(blue.rgb(), "rgb(30, 30, 220)");
        assert!(!blue.oklch().contains('/'), "{}", blue.oklch());
    }
}
