//! Splash art: the four coverage masks and the colours painted through them.

/// The masks and colours that make one splash mark.
#[derive(Debug, Clone, Copy)]
pub struct Art {
    /// Mask for the rounded tile, an 8-bit PGM image.
    pub tile: &'static [u8],
    /// Mask for the glyph, an 8-bit PGM image.
    pub glyph: &'static [u8],
    /// Mask for the progress bar, an 8-bit PGM image.
    pub bar: &'static [u8],
    /// Mask for the wordmark, an 8-bit PGM image.
    pub wordmark: &'static [u8],
    /// Tile colour as `0xRRGGBB`.
    pub tile_colour: u32,
    /// Glyph colour as `0xRRGGBB`.
    pub glyph_colour: u32,
    /// Progress bar colour as `0xRRGGBB`.
    pub bar_colour: u32,
}

impl Art {
    /// Navo's splash art: open sea and icon variant A colours from D78, as sRGB.
    pub const NAVO: Art = Art {
        tile: include_bytes!("../../../../assets/brand/splash/tile.pgm"),
        glyph: include_bytes!("../../../../assets/brand/splash/glyph.pgm"),
        bar: include_bytes!("../../../../assets/brand/splash/bar.pgm"),
        wordmark: include_bytes!("../../../../assets/brand/splash/wordmark.pgm"),
        tile_colour: 0x08_1115,
        glyph_colour: 0xE2_EDF2,
        bar_colour: 0x50_C2DF,
    };
}
