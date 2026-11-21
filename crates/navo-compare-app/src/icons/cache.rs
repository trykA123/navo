//! The Material icon pack: bounds-checked unpacking and a resvg raster cache at 1x/2x buckets.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use gpui_kit::assets::{Assets, IconName};
use gpui_kit::{App, AssetSource, Global, RenderImage};
use image::RgbaImage;
use resvg::{tiny_skia, usvg};
use sidewise_kit::primitives::{gpu_images, new_image};

use super::MaterialIcon;

/// The one bundle `scripts/import-material-icons.sh` writes: magic, u16 count, then records.
const PACK: &[u8] = include_bytes!("../../../../assets/icons/material-icons.pack");
const MAGIC: &[u8; 6] = b"SWICN1";
pub(crate) const COUNT: usize = 50;
/// CSS-pixel buckets: the 1x mark and its 2x HiDPI raster.
pub(crate) const BUCKETS: [u32; 2] = [16, 32];

/// Pack stems in `MaterialIcon` declaration order; `cache_tests` pins the pairing.
const STEMS: &str = "c console css database dll docker document exe file folder folder-config \
     folder-config-open folder-docs folder-docs-open folder-github folder-github-open folder-lib \
     folder-lib-open folder-open folder-resource folder-resource-open folder-scripts \
     folder-scripts-open folder-src folder-src-open folder-test folder-test-open freemarker git \
     go h hex html image javascript json license lock log markdown powershell python react_ts \
     readme rust svg toml typescript yaml zip";

/// Reads `len` bytes at `at` and advances it; `None` when the pack ends first.
fn take(at: &mut usize, len: usize) -> Option<&'static [u8]> {
    let end = at.checked_add(len)?;
    let found = PACK.get(*at..end)?;
    *at = end;
    Some(found)
}

/// Every packed record: the icon stem and its SVG bytes, both borrowed from the embedded bundle.
type Packed = [(&'static str, &'static [u8]); COUNT];

/// Unpacks `[u16 name len][name][u32 svg len][svg]` records; a truncated, padded or non-UTF-8 pack
/// yields `None` rather than a panic, so a damaged bundle cannot take the window down.
fn unpack() -> Option<Packed> {
    let mut at = 0;
    if take(&mut at, MAGIC.len())? != MAGIC.as_slice() {
        return None;
    }
    let count = usize::from(u16::from_le_bytes(take(&mut at, 2)?.try_into().ok()?));
    if count != COUNT {
        return None;
    }
    let mut found = Vec::with_capacity(COUNT);
    for _ in 0..count {
        let len = usize::from(u16::from_le_bytes(take(&mut at, 2)?.try_into().ok()?));
        let name = std::str::from_utf8(take(&mut at, len)?).ok()?;
        let len = usize::try_from(u32::from_le_bytes(take(&mut at, 4)?.try_into().ok()?)).ok()?;
        found.push((name, take(&mut at, len)?));
    }
    if at != PACK.len() {
        return None;
    }
    found.try_into().ok()
}

static PACKED: LazyLock<Option<Packed>> = LazyLock::new(unpack);

pub(crate) fn stem_of(icon: MaterialIcon) -> Option<&'static str> {
    STEMS.split(' ').nth(icon as usize)
}

pub(crate) fn svg_of(icon: MaterialIcon) -> Option<&'static [u8]> {
    let stem = stem_of(icon)?;
    PACKED
        .as_ref()?
        .iter()
        .find(|(name, _)| *name == stem)
        .map(|(_, svg)| *svg)
}

/// Rasterizes one packed SVG into a `side` x `side` pixmap, keeping every fill `gpui::svg` drops.
fn draw(svg: &[u8], side: u32) -> Option<tiny_skia::Pixmap> {
    let tree = usvg::Tree::from_data(svg, &usvg::Options::default()).ok()?;
    let size = tree.size();
    let (wide, tall) = (size.width(), size.height());
    if wide <= 0. || tall <= 0. {
        return None;
    }
    let mut pixmap = tiny_skia::Pixmap::new(side, side)?;
    let scale = tiny_skia::Transform::from_scale(side as f32 / wide, side as f32 / tall);
    resvg::render(&tree, scale, &mut pixmap.as_mut());
    Some(pixmap)
}

/// tiny-skia gives premultiplied RGBA; the GPU upload wants straight BGRA.
fn bgra(pixmap: tiny_skia::Pixmap) -> Vec<u8> {
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

/// One upload per (variant, bucket): at most `COUNT` x `BUCKETS` entries, kept for the process.
#[derive(Default)]
pub(crate) struct IconCache {
    images: HashMap<(MaterialIcon, u32), Arc<RenderImage>>,
}

impl Global for IconCache {}

impl IconCache {
    /// The uploaded mark for one (variant, bucket); rasterized once, then kept for the process.
    pub(crate) fn image(icon: MaterialIcon, side: u32, cx: &mut App) -> Option<Arc<RenderImage>> {
        if let Some(found) = cx.default_global::<Self>().images.get(&(icon, side)) {
            return Some(found.clone());
        }
        let pixmap = draw(svg_of(icon)?, side)?;
        let (wide, tall) = (pixmap.width(), pixmap.height());
        let pixels = RgbaImage::from_raw(wide, tall, bgra(pixmap))?;
        let image = new_image(pixels, &gpu_images(cx));
        cx.default_global::<Self>()
            .images
            .insert((icon, side), image.clone());
        Some(image)
    }
}

/// The monochrome action icons Sidewise chrome draws; each must be in `default-icons.txt`.
const CHROME_ICONS: [IconName; 2] = [IconName::Plus, IconName::Replace];

/// False when the chrome list is empty or any of its icons is absent from the embedded bundle.
pub(crate) fn chrome_icons_bundled() -> bool {
    !CHROME_ICONS.is_empty()
        && CHROME_ICONS
            .iter()
            .all(|name| Assets.load(&name.path()).is_ok_and(|found| found.is_some()))
}
