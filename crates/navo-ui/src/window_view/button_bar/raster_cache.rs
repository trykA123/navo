//! Bounded GPU raster caches for button bar icons and releasable icon picker previews.

use std::collections::VecDeque;
use std::io::Cursor;
use std::sync::Arc;

use gpui_kit::{App, Global, Hsla, ImageCacheError, Pixels, RenderImage, Rgba, SharedString};
use image::imageops::FilterType;
use image::{ImageFormat, ImageReader, Limits, RgbaImage};
use resvg::{tiny_skia, usvg};

use crate::primitives::{gpu_images, new_image, release_image};

/// Maximum number of decoded raster icon images retained in the bar icon cache.
pub(crate) const MAX_CACHE_ENTRIES: usize = 64;
/// Maximum number of tinted monochrome preview images retained in the picker cache.
pub(crate) const MAX_PICKER_CACHE_ENTRIES: usize = 96;

const MAX_DECODE_SIDE: u32 = 4096;
const MAX_DECODE_ALLOC: u64 = 64 * 1024 * 1024;

type CacheKey = (blake3::Hash, u32);
type PickerCacheKey = (blake3::Hash, u32, [u8; 4]);

struct CacheEntry<K> {
    key: K,
    image: Arc<RenderImage>,
}

struct LruCache<K, const CAP: usize> {
    entries: VecDeque<CacheEntry<K>>,
}

impl<K, const CAP: usize> Default for LruCache<K, CAP> {
    fn default() -> Self {
        Self {
            entries: VecDeque::new(),
        }
    }
}

impl<K: PartialEq, const CAP: usize> LruCache<K, CAP> {
    fn get(&mut self, key: &K) -> Option<Arc<RenderImage>> {
        let pos = self.entries.iter().position(|entry| entry.key == *key)?;
        let entry = self.entries.remove(pos)?;
        let image = Arc::clone(&entry.image);
        self.entries.push_back(entry);
        Some(image)
    }

    fn insert(&mut self, key: K, image: Arc<RenderImage>) -> Option<Arc<RenderImage>> {
        let mut evicted = None;
        if let Some(pos) = self.entries.iter().position(|entry| entry.key == key) {
            evicted = self.entries.remove(pos).map(|entry| entry.image);
        } else if self.entries.len() >= CAP {
            evicted = self.entries.pop_front().map(|entry| entry.image);
        }
        self.entries.push_back(CacheEntry { key, image });
        evicted
    }

    fn drain_all(&mut self) -> Vec<Arc<RenderImage>> {
        self.entries.drain(..).map(|entry| entry.image).collect()
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    fn contains(&self, key: &K) -> bool {
        self.entries.iter().any(|entry| entry.key == *key)
    }
}

/// Process-wide LRU cache of decoded colour SVG and PNG button bar icons.
#[derive(Default)]
pub(crate) struct RasterCache {
    inner: LruCache<CacheKey, MAX_CACHE_ENTRIES>,
}

impl Global for RasterCache {}

impl RasterCache {
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.inner.len()
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, bytes: &[u8], bucket: u32) -> bool {
        let key = (blake3::hash(bytes), bucket);
        self.inner.contains(&key)
    }
}

/// Bounded, releasable LRU cache of tinted monochrome SVG previews for the icon picker.
#[derive(Default)]
pub(crate) struct PickerRasterCache {
    inner: LruCache<PickerCacheKey, MAX_PICKER_CACHE_ENTRIES>,
}

impl Global for PickerRasterCache {}

impl PickerRasterCache {
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.inner.len()
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, svg: &[u8], bucket: u32, tint: Hsla) -> bool {
        let key = (blake3::hash(svg), bucket, hsla_to_bgra(tint));
        self.inner.contains(&key)
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

/// Returns a cached or freshly rasterized tinted `RenderImage` for a monochrome picker SVG.
pub(crate) fn tinted_svg_image(
    svg: &[u8],
    tint: Hsla,
    side: Pixels,
    scale_factor: f32,
    cx: &mut App,
) -> Result<Arc<RenderImage>, ImageCacheError> {
    let bucket = bucket_px(side, scale_factor);
    let tint_bgra = hsla_to_bgra(tint);
    let key = (blake3::hash(svg), bucket, tint_bgra);
    if let Some(hit) = cx.default_global::<PickerRasterCache>().inner.get(&key) {
        return Ok(hit);
    }

    let bgra = rasterize_tinted_svg(svg, bucket, tint_bgra)?;
    let image = new_image(bgra, &gpu_images(cx));
    let evicted = cx
        .default_global::<PickerRasterCache>()
        .inner
        .insert(key, Arc::clone(&image));
    if let Some(old) = evicted {
        release_image(old, cx);
    }
    Ok(image)
}

/// Releases every cached picker preview image from GPU atlases and empties the picker cache.
pub(crate) fn clear_picker_cache(cx: &mut App) {
    if !cx.has_global::<PickerRasterCache>() {
        return;
    }
    let images = cx.global_mut::<PickerRasterCache>().inner.drain_all();
    for image in images {
        release_image(image, cx);
    }
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
    if let Some(hit) = cx.default_global::<RasterCache>().inner.get(&key) {
        return Ok(hit);
    }

    let bgra = build(bytes, bucket)?;
    let image = new_image(bgra, &gpu_images(cx));
    let evicted = cx
        .default_global::<RasterCache>()
        .inner
        .insert(key, Arc::clone(&image));
    if let Some(old) = evicted {
        release_image(old, cx);
    }
    Ok(image)
}

fn render_svg_pixmap(svg: &[u8], bucket: u32) -> Result<tiny_skia::Pixmap, ImageCacheError> {
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
    Ok(pixmap)
}

fn rasterize_svg(svg: &[u8], bucket: u32) -> Result<RgbaImage, ImageCacheError> {
    let pixmap = render_svg_pixmap(svg, bucket)?;
    let (width, height) = (pixmap.width(), pixmap.height());
    RgbaImage::from_raw(width, height, unpremultiply_bgra(pixmap))
        .ok_or_else(|| asset_err("failed to build svg image buffer"))
}

fn rasterize_tinted_svg(
    svg: &[u8],
    bucket: u32,
    tint_bgra: [u8; 4],
) -> Result<RgbaImage, ImageCacheError> {
    let pixmap = render_svg_pixmap(svg, bucket)?;
    let (width, height) = (pixmap.width(), pixmap.height());
    RgbaImage::from_raw(width, height, tint_alpha_bgra(pixmap, tint_bgra))
        .ok_or_else(|| asset_err("failed to build tinted svg image buffer"))
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

fn tint_alpha_bgra(pixmap: tiny_skia::Pixmap, [b, g, r, tint_a]: [u8; 4]) -> Vec<u8> {
    let mut data = pixmap.take();
    let tint_alpha = u16::from(tint_a);
    for pixel in data.as_chunks_mut::<4>().0 {
        let alpha = u16::from(pixel[3]);
        if alpha == 0 || tint_alpha == 0 {
            *pixel = [0, 0, 0, 0];
            continue;
        }
        let out_alpha = ((alpha * tint_alpha + 127) / 255) as u8;
        if out_alpha == 0 {
            *pixel = [0, 0, 0, 0];
        } else {
            *pixel = [b, g, r, out_alpha];
        }
    }
    data
}

fn hsla_to_bgra(tint: Hsla) -> [u8; 4] {
    let rgba = Rgba::from(tint);
    let to_u8 = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u8;
    [to_u8(rgba.b), to_u8(rgba.g), to_u8(rgba.r), to_u8(rgba.a)]
}

fn rgba_to_bgra(rgba: &mut RgbaImage) {
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
}

fn asset_err(msg: impl Into<SharedString>) -> ImageCacheError {
    ImageCacheError::Asset(msg.into())
}
