//! The Stream's GPU thumbnails: the images of the cards near the view, uploaded when their
//! detail arrives and freed when a card leaves the view, changes or goes (D26.3). The bytes come
//! from the image detail; nothing here draws them (A9b does).

use std::collections::HashMap;
use std::sync::Arc;

use gpui_kit::{App, RenderImage};
use image::RgbaImage;

use crate::compare_diff::{CardDetail, StreamModel, Thumb};
use crate::primitives::{gpu_images, new_image, release_image};

/// One card's uploaded textures, tagged with the detail they were made from.
struct Shown {
    detail: Arc<CardDetail>,
    left: Option<Arc<RenderImage>>,
    right: Option<Arc<RenderImage>>,
}

/// The thumbnails the Stream keeps on the GPU, by item id (D26.3). As visible as the
/// `StreamScreen` field that holds it, and no more.
#[derive(Default)]
pub(in crate::window_view::compare_screen) struct Thumbs {
    shown: HashMap<u64, Shown>,
}

impl Thumbs {
    /// Uploads the thumbnails of every id in `keep` whose image detail is not shown yet, and
    /// frees those that left `keep`, were evicted or were replaced (D26.3). A side whose bytes
    /// are not an image of its size is skipped and draws nothing.
    pub(super) fn sync(&mut self, model: &StreamModel, keep: &[u64], cx: &mut App) {
        for id in self.gone(model, keep) {
            self.free(id, cx);
        }
        for &id in keep {
            if self.shown.contains_key(&id) {
                continue;
            }
            let Some(held) = model.detail(id) else {
                continue;
            };
            let CardDetail::Image(card) = &**held else {
                continue;
            };
            let shown = Shown {
                detail: Arc::clone(held),
                left: card.left.as_ref().and_then(|thumb| upload(thumb, cx)),
                right: card.right.as_ref().and_then(|thumb| upload(thumb, cx)),
            };
            self.shown.insert(id, shown);
        }
    }

    /// The uploaded textures of item `id`'s card, `None` on each side it does not show.
    pub(super) fn pair(&self, id: u64) -> (Option<&Arc<RenderImage>>, Option<&Arc<RenderImage>>) {
        match self.shown.get(&id) {
            Some(shown) => (shown.left.as_ref(), shown.right.as_ref()),
            None => (None, None),
        }
    }

    /// Frees every texture the Stream holds: it closes, or a new Stream replaces it (D26.3).
    pub(super) fn release_all(&mut self, cx: &mut App) {
        for id in self.shown.keys().copied().collect::<Vec<u64>>() {
            self.free(id, cx);
        }
    }

    /// The shown ids that are not in `keep` any more, or whose stored detail is no longer the one
    /// the model holds — an eviction or a new snapshot (D26.2, D26.3).
    fn gone(&self, model: &StreamModel, keep: &[u64]) -> Vec<u64> {
        let mut gone = Vec::new();
        for (id, shown) in &self.shown {
            let current = model
                .detail(*id)
                .is_some_and(|held| Arc::ptr_eq(held, &shown.detail));
            if !keep.contains(id) || !current {
                gone.push(*id);
            }
        }
        gone
    }

    /// Frees both textures of `id` and drops its entry.
    fn free(&mut self, id: u64, cx: &mut App) {
        let Some(shown) = self.shown.remove(&id) else {
            return;
        };
        for image in [shown.left, shown.right].into_iter().flatten() {
            release_image(image, cx);
        }
    }
}

/// One card thumbnail as a GPU texture; `None` when its bytes are not `width` × `height` pixels.
fn upload(thumb: &Thumb, cx: &mut App) -> Option<Arc<RenderImage>> {
    let pixels = RgbaImage::from_raw(thumb.width, thumb.height, thumb.bgra.to_vec())?;
    Some(new_image(pixels, &gpu_images(cx)))
}
