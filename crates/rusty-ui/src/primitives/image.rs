use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};

use gpui_kit::{App, ImageId, RenderImage};
use image::{Frame, RgbaImage};
use rusty_diag::components::{MemoryUse, Tally};

use super::tally::{AppHeld, AppTally};

pub(crate) static GPU_IMAGES: Tally = Tally::new("gpu images");

static LIVE: LazyLock<Mutex<HashMap<ImageId, AppHeld>>> = LazyLock::new(Mutex::default);

fn live() -> MutexGuard<'static, HashMap<ImageId, AppHeld>> {
    LIVE.lock().unwrap_or_else(PoisonError::into_inner)
}

pub fn gpu_images(cx: &mut App) -> AppTally {
    AppTally::of(&GPU_IMAGES, cx)
}

pub fn new_image(bgra: RgbaImage, gpu: &AppTally) -> Arc<RenderImage> {
    let bytes = bgra.as_raw().len() as u64;
    let image = Arc::new(RenderImage::new(vec![Frame::new(bgra)]));
    let held = gpu.hold(&GPU_IMAGES, MemoryUse { bytes, items: 1 });
    live().insert(image.id, held);
    image
}

// A window being updated is leased and skipped by `drop_image`, so its atlas would keep the texture.
pub fn release_image(image: Arc<RenderImage>, cx: &mut App) {
    cx.defer(move |cx| {
        let id = image.id;
        cx.drop_image(image, None);
        live().remove(&id);
    });
}

#[cfg(test)]
mod tests {
    use gpui_kit::TestAppContext;

    use super::*;
    use crate::primitives::tally;

    #[gpui_kit::test]
    fn an_image_is_counted_until_it_is_released(cx: &mut TestAppContext) {
        let gpu = cx.update(gpu_images);
        let image = new_image(RgbaImage::new(4, 2), &gpu);
        assert_eq!(
            cx.read(|cx| tally::read(&GPU_IMAGES, cx)),
            MemoryUse {
                bytes: 32,
                items: 1
            }
        );
        cx.update(|cx| release_image(image, cx));
        cx.run_until_parked();
        assert_eq!(
            cx.read(|cx| tally::read(&GPU_IMAGES, cx)),
            MemoryUse::default()
        );
    }
}
