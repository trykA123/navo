//! The state of the image diff body (V40, from I4): which of the three views is open, where the
//! overlay wipe sits, and what the app's scores and its D20 fallbacks let the screen say.

use std::sync::Arc;

use super::hooks::{ImageLimit, ImageMask, ImageScores};

/// One way to read an image pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageMode {
    /// The two pictures, each in its own half.
    SideBySide,
    /// One picture over the other, divided by a wipe the reader moves.
    Overlay,
    /// The difference mask on its own.
    Mask,
}

impl ImageMode {
    /// The position of this view in the picker row.
    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Self::SideBySide => 0,
            Self::Overlay => 1,
            Self::Mask => 2,
        }
    }

    /// The label of this view's entry.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::SideBySide => "compare-image-side-by-side",
            Self::Overlay => "compare-image-overlay",
            Self::Mask => "compare-image-mask",
        }
    }
}

/// One wipe step, a twentieth of the stage.
pub const SPLIT_STEP: f32 = 0.05;
/// The longest side of a drawn difference mask, in pixels: a heat map needs no more.
pub const MASK_SIDE: u32 = 2048;

/// What the file diff shows of one image pair.
#[derive(Debug, Clone)]
pub struct ImageBody {
    mode: ImageMode,
    split: f32,
    scores: Option<ImageScores>,
    mask: Option<Arc<ImageMask>>,
    /// Whether the parts beyond the pair itself show: the picker, the wipe, the scores.
    rich: bool,
}

impl ImageBody {
    /// A pair that has not been scored yet: side by side, the wipe in the middle.
    #[must_use]
    pub fn new(rich: bool) -> Self {
        Self {
            mode: ImageMode::SideBySide,
            split: 0.5,
            scores: None,
            mask: None,
            rich,
        }
    }

    /// Takes the app's scores and mask; false when nothing shown changes.
    pub fn set_result(&mut self, scores: ImageScores, mask: Option<Arc<ImageMask>>) -> bool {
        let changed = self.scores.as_ref() != Some(&scores) || self.mask != mask;
        if !changed {
            return false;
        }
        self.scores = Some(scores);
        self.mask = mask;
        if !self.can_show(self.mode) {
            self.mode = ImageMode::SideBySide;
        }
        true
    }

    /// Turns the extra parts on or off; leaving Rich closes their views.
    pub fn set_rich(&mut self, rich: bool) -> bool {
        if self.rich == rich {
            return false;
        }
        self.rich = rich;
        if !rich {
            self.mode = ImageMode::SideBySide;
        }
        true
    }

    /// The open view.
    #[must_use]
    pub fn mode(&self) -> ImageMode {
        self.mode
    }

    /// Where the wipe sits: the share of the stage that shows the left picture.
    #[must_use]
    pub fn split(&self) -> f32 {
        self.split
    }

    /// Opens `mode`; false when the pair cannot show it or it already was open.
    pub fn set_mode(&mut self, mode: ImageMode) -> bool {
        if !self.can_show(mode) || self.mode == mode {
            return false;
        }
        self.mode = mode;
        true
    }

    /// Whether the parts beyond the pair itself show: the picker, the scores.
    #[must_use]
    pub fn parts(&self) -> bool {
        self.rich
    }

    /// Whether `mode` has anything to draw. Pictures of different sizes cannot be laid over
    /// one another, so a size mismatch offers no overlay.
    #[must_use]
    pub fn can_show(&self, mode: ImageMode) -> bool {
        let mismatch = matches!(self.scores, Some(ImageScores::SizeMismatch { .. }));
        match mode {
            ImageMode::SideBySide => true,
            ImageMode::Overlay => self.rich && !mismatch,
            ImageMode::Mask => self.rich && self.mask.is_some(),
        }
    }

    /// Moves the wipe one step forward or back; false when it stayed where it was.
    pub fn nudge(&mut self, forward: bool) -> bool {
        let step = if forward { SPLIT_STEP } else { -SPLIT_STEP };
        self.set_split(self.split + step)
    }

    /// Puts the wipe at `at`, clamped to the stage; false when that is where it already was.
    pub fn set_split(&mut self, at: f32) -> bool {
        let to = at.clamp(0., 1.);
        let changed = to != self.split;
        self.split = to;
        changed
    }

    /// The app's result, once it sent one.
    #[must_use]
    pub fn scores(&self) -> Option<&ImageScores> {
        self.scores.as_ref()
    }

    /// The difference mask, for the view to draw.
    #[must_use]
    pub fn mask(&self) -> Option<&Arc<ImageMask>> {
        self.mask.as_ref()
    }

    /// Similarity as a whole percent, Rich only: the structural score is the one to lead with.
    #[must_use]
    pub fn similarity(&self) -> Option<u32> {
        let scores = self.scores.as_ref()?;
        if !self.rich {
            return None;
        }
        similarity_of(scores)
    }

    /// Why the pair has no scores, as a Fluent id; `None` when it has them.
    #[must_use]
    pub fn notice(&self) -> Option<&'static str> {
        let scores = self.scores.as_ref()?;
        notice_of(scores)
    }

    /// Whether the pair is worth reading and decoding for the view: a hashed or oversized result
    /// says the files are past the image caps, so the screen must not try (D20).
    #[must_use]
    pub fn wants_sides(&self) -> bool {
        match self.scores.as_ref() {
            Some(
                ImageScores::Scored { .. }
                | ImageScores::SizeMismatch { .. }
                | ImageScores::Unsupported { .. },
            ) => true,
            Some(ImageScores::HashOnly { .. } | ImageScores::TooLarge { .. }) | None => false,
        }
    }
}

/// Similarity as a whole percent for any card: the structural score of a scored pair,
/// rounded and clamped to `0..=100`; `None` for every D20 fallback.
#[must_use]
pub fn similarity_of(scores: &ImageScores) -> Option<u32> {
    let ImageScores::Scored { ssim, .. } = scores else {
        return None;
    };
    let percent = (*ssim * 100.).round().clamp(0., 100.);
    // The value is a rounded percent already clamped to 0..=100.
    Some(percent as u32)
}

/// Why a pair has no scores, as a Fluent id, for any card: `None` when it has them.
#[must_use]
pub fn notice_of(scores: &ImageScores) -> Option<&'static str> {
    match scores {
        ImageScores::Scored { .. } => None,
        ImageScores::HashOnly { .. } => Some("compare-image-note-hash"),
        ImageScores::SizeMismatch { .. } => Some("compare-image-note-sizes"),
        ImageScores::Unsupported { .. } => Some("compare-image-note-format"),
        ImageScores::TooLarge { reason, .. } => Some(match reason {
            ImageLimit::Encoded => "compare-image-note-encoded",
            ImageLimit::Pixels => "compare-image-note-pixels",
            ImageLimit::Decode => "compare-image-note-decode",
        }),
    }
}

/// The mask as grey pixels, box-max-sampled to `MASK_SIDE`: shrinking a heat map must not drop a
/// one-pixel difference, so each drawn pixel is the strongest in its block.
#[must_use]
pub fn mask_pixels(mask: &ImageMask) -> image::RgbaImage {
    use image::Rgba;
    let step = usize::try_from(mask.width.max(mask.height).div_ceil(MASK_SIDE).max(1))
        .unwrap_or(usize::MAX);
    let wide = mask.width as usize;
    let (out_w, out_h) = (
        (wide / step).max(1).min(mask.width as usize),
        (mask.height as usize / step)
            .max(1)
            .min(mask.height as usize),
    );
    let mut out = image::RgbaImage::new(out_w as u32, out_h as u32);
    for (x, y, pixel) in out.enumerate_pixels_mut() {
        let (x, y) = (x as usize * step, y as usize * step);
        let mut strongest = 0u8;
        for row in y..(y + step).min(mask.height as usize) {
            for column in x..(x + step).min(wide) {
                let at = row.saturating_mul(wide).saturating_add(column);
                strongest = strongest.max(mask.pixels.get(at).copied().unwrap_or(0));
            }
        }
        *pixel = Rgba([strongest, strongest, strongest, 255]);
    }
    out
}
