//! Image pair result types the app sends and the image body reads (V40, D20).

/// Pixel dimensions of one image side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageSize {
    pub width: u32,
    pub height: u32,
}

/// How far two images' alpha channels differ; `differs` is true above zero.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageAlpha {
    pub max_delta: u8,
    pub mean_delta: f64,
    pub differs: bool,
}

/// Why an image pair was not compared in full.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageLimit {
    /// The encoded file is over the 64 MiB read cap.
    Encoded,
    /// A side is over the 64 MP cap, or a WebP is over its 48 MP cap.
    Pixels,
    /// A side does not fit the decoder's allocation limits.
    Decode,
}

/// An image pair's result: full scores, or the cheaper fallbacks of D20.
#[derive(Debug, Clone, PartialEq)]
pub enum ImageScores {
    /// Structural, RGB and histogram scores; 1.0 is identical.
    Scored {
        ssim: f64,
        rgb_min_rms_similarity: f64,
        luma_hellinger_similarity: f64,
        alpha: Option<ImageAlpha>,
    },
    /// A side is over 64 MP, so only the raw bytes were compared.
    HashOnly { equal: bool },
    /// Different pixel dimensions prevent scoring.
    SizeMismatch { left: ImageSize, right: ImageSize },
    /// The pair cannot be read or decoded within the image limits; `equal` is the byte hash.
    TooLarge { reason: ImageLimit, equal: bool },
    /// The file names a format the image decoder does not support, such as SVG.
    Unsupported { format: &'static str },
}

/// A grayscale difference mask: 0 means equal and 255 the largest difference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageMask {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}
