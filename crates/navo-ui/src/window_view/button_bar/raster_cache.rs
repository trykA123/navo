//! Bounded 64-entry GPU raster cache for colour SVG and PNG button bar icons.

use std::collections::VecDeque;
use std::io::Cursor;
use std::sync::Arc;

use gpui_kit::{App, Global, ImageCacheError, Pixels, RenderImage, SharedString};
use image::imageops::FilterType;
use image::{ImageFormat, ImageReader, Limits, RgbaImage};
use resvg::{tiny_skia, usvg};

use crate::primitives::{gpu_images, new_image, release_image};

/// Maximum number of decoded raster icon images retained in the global cache.
pub(crate) const MAX_CACHE_ENTRIES: usize = 64;

const MAX_DECODE_SIDE: u32 = 4096;
const MAX_DECODE_ALLOC: u64 = 64 * 1024 * 1024;

type CacheKey = (blake3::Hash, u32);

struct CacheEntry {
    key: CacheKey,
    image: Arc<RenderImage>,
}

/// Process-wide LRU cache of decoded colour SVG and PNG button bar icons.
#[derive(Default)]
pub(crate) struct RasterCache {
    entries: VecDeque<CacheEntry>,
}

impl Global for RasterCache {}

impl RasterCache {
    fn get(&mut self, key: CacheKey) -> Option<Arc<RenderImage>> {
        let pos = self.entries.iter().position(|entry| entry.key == key)?;
        let entry = self.entries.remove(pos)?;
        let image = Arc::clone(&entry.image);
        self.entries.push_back(entry);
        Some(image)
    }

    fn insert(&mut self, key: CacheKey, image: Arc<RenderImage>) -> Option<Arc<RenderImage>> {
        let mut evicted = None;
        if let Some(pos) = self.entries.iter().position(|entry| entry.key == key) {
            evicted = self.entries.remove(pos).map(|entry| entry.image);
        } else if self.entries.len() >= MAX_CACHE_ENTRIES {
            evicted = self.entries.pop_front().map(|entry| entry.image);
        }
        self.entries.push_back(CacheEntry { key, image });
        evicted
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, bytes: &[u8], bucket: u32) -> bool {
        let key = (blake3::hash(bytes), bucket);
        self.entries.iter().any(|entry| entry.key == key)
    }
}

/// Snaps `(side, scale_factor)` to the 1x, 1.5x, or 2x physical pixel bucket.
#[must_use]
pub(crate) fn bucket_px(side: Pixels, scale_factor: f32) -> u32 {
    let scale = if !scale_factor.is_finite() || scale_factor <= 1.125 {
        1.0
    } else if scale_factor <= 1.625 {
        1.5
    } else {
        2.0
    };
    (side.as_f32().max(1.0) * scale).round().max(1.0) as u32
}

/// Returns a cached or freshly rasterized `RenderImage` for a colour SVG icon.
pub(crate) fn svg_image(
    svg: &[u8],
    side: Pixels,
    scale_factor: f32,
    cx: &mut App,
) -> Result<Arc<RenderImage>, ImageCacheError> {
    cached_or_build(svg, side, scale_factor, cx, rasterize_svg)
}

/// Returns a cached or freshly decoded `RenderImage` for a PNG icon.
pub(crate) fn png_image(
    png: &[u8],
    side: Pixels,
    scale_factor: f32,
    cx: &mut App,
) -> Result<Arc<RenderImage>, ImageCacheError> {
    cached_or_build(png, side, scale_factor, cx, decode_png)
}

fn cached_or_build(
    bytes: &[u8],
    side: Pixels,
    scale_factor: f32,
    cx: &mut App,
    build: impl FnOnce(&[u8], u32) -> Result<RgbaImage, ImageCacheError>,
) -> Result<Arc<RenderImage>, ImageCacheError> {
    let bucket = bucket_px(side, scale_factor);
    let key = (blake3::hash(bytes), bucket);
    if let Some(hit) = cx.default_global::<RasterCache>().get(key) {
        return Ok(hit);
    }

    let bgra = build(bytes, bucket)?;
    let image = new_image(bgra, &gpu_images(cx));
    let evicted = cx
        .default_global::<RasterCache>()
        .insert(key, Arc::clone(&image));
    if let Some(old) = evicted {
        release_image(old, cx);
    }
    Ok(image)
}

fn rasterize_svg(svg: &[u8], bucket: u32) -> Result<RgbaImage, ImageCacheError> {
    let tree = usvg::Tree::from_data(svg, &usvg::Options::default())
        .map_err(|err| asset_err(format!("invalid svg icon: {err}")))?;
    let size = tree.size();
    let (wide, tall) = (size.width(), size.height());
    if !wide.is_finite() || !tall.is_finite() || wide <= 0.0 || tall <= 0.0 {
        return Err(asset_err("svg icon has zero size"));
    }

    let scale = bucket as f32 / wide.max(tall);
    let target_w = (wide * scale).round().max(1.0) as u32;
    let target_h = (tall * scale).round().max(1.0) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(target_w, target_h)
        .ok_or_else(|| asset_err("failed to allocate svg pixmap"))?;
    let transform =
        tiny_skia::Transform::from_scale(target_w as f32 / wide, target_h as f32 / tall);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    RgbaImage::from_raw(target_w, target_h, unpremultiply_bgra(pixmap))
        .ok_or_else(|| asset_err("failed to build svg image buffer"))
}

fn decode_png(png: &[u8], bucket: u32) -> Result<RgbaImage, ImageCacheError> {
    let mut reader = ImageReader::with_format(Cursor::new(png), ImageFormat::Png);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DECODE_SIDE);
    limits.max_image_height = Some(MAX_DECODE_SIDE);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    reader.limits(limits);

    let decoded = reader
        .decode()
        .map_err(|err| asset_err(format!("invalid png icon: {err}")))?;
    if decoded.width() == 0 || decoded.height() == 0 {
        return Err(asset_err("png icon has zero dimension"));
    }

    let scaled = if decoded.width() > bucket || decoded.height() > bucket {
        decoded.resize(bucket, bucket, FilterType::Lanczos3)
    } else {
        decoded
    };
    let mut rgba = scaled.into_rgba8();
    rgba_to_bgra(&mut rgba);
    Ok(rgba)
}

fn unpremultiply_bgra(pixmap: tiny_skia::Pixmap) -> Vec<u8> {
    let mut data = pixmap.take();
    for pixel in data.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
        let alpha = u16::from(pixel[3]);
        if alpha == 0 || alpha == 255 {
            continue;
        }
        for channel in &mut pixel[..3] {
            *channel = (u16::from(*channel) * 255 / alpha).min(255) as u8;
        }
    }
    data
}

fn rgba_to_bgra(rgba: &mut RgbaImage) {
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
}

fn asset_err(msg: impl Into<SharedString>) -> ImageCacheError {
    ImageCacheError::Asset(msg.into())
}
