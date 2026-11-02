use std::borrow::Cow;

use ::image::{DynamicImage, GrayImage, Luma, RgbImage, RgbaImage};
use image_compare::{Algorithm, CompareError, Metric, Similarity};

pub const MAX_PIXELS: u64 = 64_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageSize {
    pub width: u32,
    pub height: u32,
}

impl ImageSize {
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    #[must_use]
    pub fn pixels(self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    #[must_use]
    pub fn is_too_large(self) -> bool {
        self.pixels() > MAX_PIXELS
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageScores {
    /// MSSIM over RGB in [-1, 1]; 1.0 is identical.
    pub ssim: f64,
    pub rgb_min_rms_similarity: f64,
    pub luma_hellinger_similarity: f64,
    pub alpha: Option<AlphaDiff>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AlphaDiff {
    pub max_delta: u8,
    pub mean_delta: f64,
    pub differs: bool,
}

#[derive(Debug)]
pub enum ImageOutcome {
    Scored {
        scores: ImageScores,
        diff: DynamicImage,
    },
    HashOnly,
    SizeMismatch {
        first: ImageSize,
        second: ImageSize,
    },
}

#[derive(Debug)]
pub enum ImageError {
    Empty,
    Comparison(CompareError),
}

impl std::fmt::Display for ImageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => formatter.write_str("an image has no pixels"),
            Self::Comparison(error) => write!(formatter, "image comparison failed: {error}"),
        }
    }
}

impl std::error::Error for ImageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Empty => None,
            Self::Comparison(error) => Some(error),
        }
    }
}

impl From<CompareError> for ImageError {
    fn from(error: CompareError) -> Self {
        Self::Comparison(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Precheck {
    Score,
    HashOnly,
    SizeMismatch,
    Empty,
}

pub fn compare(first: &DynamicImage, second: &DynamicImage) -> Result<ImageOutcome, ImageError> {
    let first_size = ImageSize::new(first.width(), first.height());
    let second_size = ImageSize::new(second.width(), second.height());
    match precheck(first_size, second_size) {
        Precheck::HashOnly => Ok(ImageOutcome::HashOnly),
        Precheck::SizeMismatch => Ok(ImageOutcome::SizeMismatch {
            first: first_size,
            second: second_size,
        }),
        Precheck::Empty => Err(ImageError::Empty),
        Precheck::Score => score(first, second),
    }
}

fn precheck(first: ImageSize, second: ImageSize) -> Precheck {
    if first.is_too_large() || second.is_too_large() {
        return Precheck::HashOnly;
    }
    if first != second {
        return Precheck::SizeMismatch;
    }
    if first.pixels() == 0 {
        return Precheck::Empty;
    }
    Precheck::Score
}

fn score(first: &DynamicImage, second: &DynamicImage) -> Result<ImageOutcome, ImageError> {
    let (ssim, rgb_min_rms_similarity, mut diff) = {
        let first_rgb = rgb8(first);
        let second_rgb = rgb8(second);
        structural_scores(first_rgb.as_ref(), second_rgb.as_ref())?
    };
    let first_luma = luma8(first);
    let second_luma = luma8(second);
    let luma_hellinger_similarity =
        histogram_similarity(first_luma.as_ref(), second_luma.as_ref())?;
    let alpha = (first.color().has_alpha() || second.color().has_alpha()).then(|| {
        let first_rgba = rgba8(first);
        let second_rgba = rgba8(second);
        alpha_difference(first_rgba.as_ref(), second_rgba.as_ref(), &mut diff)
    });
    Ok(ImageOutcome::Scored {
        scores: ImageScores {
            ssim,
            rgb_min_rms_similarity,
            luma_hellinger_similarity,
            alpha,
        },
        diff: diff.into(),
    })
}

fn rgb8(image: &DynamicImage) -> Cow<'_, RgbImage> {
    match image.as_rgb8() {
        Some(rgb) => Cow::Borrowed(rgb),
        None => Cow::Owned(image.to_rgb8()),
    }
}

fn luma8(image: &DynamicImage) -> Cow<'_, GrayImage> {
    match image.as_luma8() {
        Some(luma) => Cow::Borrowed(luma),
        None => Cow::Owned(image.to_luma8()),
    }
}

fn rgba8(image: &DynamicImage) -> Cow<'_, RgbaImage> {
    match image.as_rgba8() {
        Some(rgba) => Cow::Borrowed(rgba),
        None => Cow::Owned(image.to_rgba8()),
    }
}

fn structural_scores(
    first: &RgbImage,
    second: &RgbImage,
) -> Result<(f64, f64, GrayImage), ImageError> {
    let (rgb_min_rms_similarity, diff) = {
        let similarity =
            image_compare::rgb_similarity_structure(&Algorithm::RootMeanSquared, first, second)?;
        (similarity.score, difference_mask(&similarity))
    };
    let ssim =
        image_compare::rgb_similarity_structure(&Algorithm::MSSIMSimple, first, second)?.score;
    Ok((ssim, rgb_min_rms_similarity, diff))
}

fn difference_mask(similarity: &Similarity) -> GrayImage {
    let map = similarity.image.to_color_map().into_rgb8();
    let mut mask = GrayImage::new(map.width(), map.height());
    for (mask_pixel, map_pixel) in mask.pixels_mut().zip(map.pixels()) {
        let difference = map_pixel
            .0
            .iter()
            .map(|channel| u8::MAX - *channel)
            .fold(0, u8::max);
        *mask_pixel = Luma([difference]);
    }
    mask
}

fn alpha_difference(first: &RgbaImage, second: &RgbaImage, mask: &mut GrayImage) -> AlphaDiff {
    let mut max_delta = 0;
    let mut total = 0u64;
    for ((mask_pixel, first_pixel), second_pixel) in
        mask.pixels_mut().zip(first.pixels()).zip(second.pixels())
    {
        let delta = first_pixel[3].abs_diff(second_pixel[3]);
        mask_pixel[0] = mask_pixel[0].max(delta);
        max_delta = max_delta.max(delta);
        total += u64::from(delta);
    }
    let pixels = u64::from(mask.width()) * u64::from(mask.height());
    let mean_delta = if pixels == 0 {
        0.0
    } else {
        total as f64 / pixels as f64
    };
    AlphaDiff {
        max_delta,
        mean_delta,
        differs: max_delta > 0,
    }
}

fn histogram_similarity(first: &GrayImage, second: &GrayImage) -> Result<f64, ImageError> {
    let distance = image_compare::gray_similarity_histogram(Metric::Hellinger, first, second)?;
    Ok((1.0 - distance).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests;
