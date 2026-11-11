//! The image diff body (V40, from I4): the two pictures side by side, an overlay wipe with a
//! slider, and the difference mask. The sides are read and decoded on a worker through the
//! window's file hooks; the scores and the mask come from the app (D24, D58).

mod glue;
mod view;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::{Bounds, Context, Pixels, RenderImage, Task};
use image::RgbaImage;
use navo_core::Location;
use navo_l10n::{t, tr};

use crate::compare_diff::image_body::{self, ImageBody, ImageMode};
use crate::compare_diff::{ImageMask, ImageScores, PairRequest};
use crate::prelude::*;
use crate::primitives::{gpu_images, new_image, release_image};
use crate::window_view::viewer::{IMAGE_LIMIT, Picture, decode_image};
use crate::window_view::{FileHooks, NavoWindow};

#[cfg(test)]
pub(in crate::window_view) use glue::Probe;
pub(in crate::window_view::compare_screen) use view::{body, image_keys};

/// The label of a pair where a side could not be read or decoded.
const UNREADABLE: &str = "compare-image-note-unreadable";

/// Where the last frame placed the pictures, so a pointer x becomes a fraction of them.
type Stage = Rc<RefCell<Option<Bounds<Pixels>>>>;

/// What one side of the pair is doing.
enum Slot {
    /// Nothing asked for yet, or no such side.
    Idle,
    /// Reading and decoding on a worker.
    Loading,
    /// Ready to draw, with its size in pixels.
    Ready(Arc<RenderImage>, u32, u32),
    /// The read or the decode said no.
    Failed,
}

/// One side as the worker sent it back.
enum Loaded {
    /// Decoded and ready for the GPU.
    Picture(Picture),
    /// There, but unreadable or not a picture.
    Failed,
}

/// What one worker run brought back: the two sides.
struct Found {
    sides: [Option<Loaded>; 2],
}

/// What the pointer did over the wipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::window_view) enum Pointer {
    /// The left button went down.
    Down,
    /// The pointer moved; true while the left button is still held.
    Move(bool),
    /// The left button came up, inside the wipe or outside it.
    Up,
}

/// The image pair as the file diff shows it.
pub(in crate::window_view) struct Pictures {
    /// Which view is open, where the wipe sits, and what the scores say.
    model: ImageBody,
    /// The two sides, left then right.
    slots: [Slot; 2],
    /// Where to read each side from, and the name to decode it as.
    sources: [(Option<Location>, String); 2],
    /// The mask the app sent, and the picture built from it.
    mask: Option<Arc<ImageMask>>,
    mask_image: Option<Arc<RenderImage>>,
    /// The job that read the pair, and the one that builds the mask picture; dropping the
    /// body drops them.
    load: Option<Task<()>>,
    mask_job: Option<Task<()>>,
    /// Pictures a newer result replaced; the next poll gives them back to the GPU.
    garbage: Vec<Arc<RenderImage>>,
    /// Where the pictures were placed on the last frame.
    stage: Stage,
    /// Set while the pointer drags the wipe.
    dragging: bool,
}

impl Pictures {
    /// A body for `request`; `rich` says whether the parts beyond the pair show.
    pub(in crate::window_view) fn new(request: &PairRequest, rich: bool) -> Self {
        let name = request
            .path
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(&request.path)
            .to_owned();
        let side = |at: &Option<Location>| (at.clone(), name.clone());
        Self {
            model: ImageBody::new(rich),
            slots: [Slot::Idle, Slot::Idle],
            sources: [side(&request.left), side(&request.right)],
            mask: None,
            mask_image: None,
            load: None,
            mask_job: None,
            garbage: Vec::new(),
            stage: Stage::default(),
            dragging: false,
        }
    }

    /// Takes the app's scores and mask; false when nothing shown changes.
    pub(super) fn set_result(&mut self, scores: ImageScores, mask: Option<ImageMask>) -> bool {
        let arc = mask.map(Arc::new);
        let changed = self.model.set_result(scores, arc.clone());
        if self.mask != arc {
            self.mask = arc;
            self.mask_job = None;
            self.garbage.extend(self.mask_image.take());
        }
        changed
    }

    /// Follows the View menu's Simple / Rich choice.
    pub(super) fn set_rich(&mut self, rich: bool) {
        self.model.set_rich(rich);
    }

    /// The open view.
    pub(super) fn mode(&self) -> ImageMode {
        self.model.mode()
    }

    /// Opens `mode`; false when the pair cannot show it or it already was open.
    pub(super) fn set_mode(&mut self, mode: ImageMode) -> bool {
        self.model.set_mode(mode)
    }

    /// Whether the pair can show `mode` at all.
    pub(super) fn shows(&self, mode: ImageMode) -> bool {
        self.model.can_show(mode)
    }

    /// Whether the parts beyond the pair itself show.
    pub(super) fn parts(&self) -> bool {
        self.model.parts()
    }

    /// Where the wipe sits, as a fraction of the stage.
    pub(super) fn split(&self) -> f32 {
        self.model.split()
    }

    /// Why the pair shows less than it could, as a Fluent id.
    pub(super) fn notice(&self) -> Option<&'static str> {
        if self.slots.iter().any(|slot| matches!(slot, Slot::Failed)) {
            return Some(UNREADABLE);
        }
        self.model.notice()
    }

    /// The status line: the similarity, or the reason there is none.
    pub(super) fn status(&self) -> Option<SharedString> {
        if let Some(id) = self.notice() {
            return Some(tr(id).into());
        }
        match self.model.similarity() {
            Some(percent) => Some(t!("compare-image-similar", percent = percent).into()),
            None if self.model.scores().is_none() => Some(tr("compare-screen-comparing").into()),
            None => None,
        }
    }

    /// The ready picture of one side.
    pub(super) fn picture(&self, at: usize) -> Option<Arc<RenderImage>> {
        match &self.slots[at] {
            Slot::Ready(image, _, _) => Some(image.clone()),
            _ => None,
        }
    }

    /// The size to fit the wipe's box to: the left side, or the right with no left.
    pub(super) fn side_size(&self) -> Option<(f32, f32)> {
        let size = |slot: &Slot| match slot {
            Slot::Ready(_, width, height) => Some((*width as f32, *height as f32)),
            _ => None,
        };
        size(&self.slots[0]).or_else(|| size(&self.slots[1]))
    }

    /// One key of the image body: `1`..`3` open a view, the arrows and Home/End move the wipe.
    /// `None` for a key that is not the body's; `Some(changed)` for one that is.
    fn key(&mut self, key: &str) -> Option<bool> {
        let mode = match key {
            "1" => Some(ImageMode::SideBySide),
            "2" => Some(ImageMode::Overlay),
            "3" => Some(ImageMode::Mask),
            _ => None,
        };
        if let Some(mode) = mode {
            return self.parts().then(|| self.model.set_mode(mode));
        }
        if !self.parts() || self.model.mode() != ImageMode::Overlay {
            return None;
        }
        match key {
            "left" => Some(self.model.nudge(false)),
            "right" => Some(self.model.nudge(true)),
            "home" => Some(self.model.set_split(0.)),
            "end" => Some(self.model.set_split(1.)),
            _ => None,
        }
    }

    /// Follows the pointer over the wipe; true when the wipe moved. A move with the button
    /// up ends a drag that was released outside the stage.
    pub(super) fn pointer(&mut self, at: Pointer, x: Pixels) -> bool {
        match at {
            Pointer::Down => self.dragging = true,
            Pointer::Move(held) if self.dragging && !held => self.dragging = false,
            Pointer::Move(_) => {}
            Pointer::Up => self.dragging = false,
        }
        if !self.dragging {
            return false;
        }
        let Some(placed) = *self.stage.borrow() else {
            return false;
        };
        let width = f32::from(placed.size.width).max(1.);
        self.model
            .set_split((f32::from(x) - f32::from(placed.origin.x)) / width)
    }

    /// Starts the jobs that read the sides and build the mask picture; true when one started.
    fn ensure_load(&mut self, files: &FileHooks, cx: &mut Context<NavoWindow>) -> bool {
        let sides = self.load_sides(files, cx);
        let mask = self.build_mask(cx);
        sides | mask
    }

    /// Reads both sides once the scores say the pair is worth decoding.
    fn load_sides(&mut self, files: &FileHooks, cx: &mut Context<NavoWindow>) -> bool {
        if !self.model.wants_sides() || self.load.is_some() {
            return false;
        }
        for slot in &mut self.slots {
            if matches!(slot, Slot::Idle) {
                *slot = Slot::Loading;
            }
        }
        let (hooks, sources) = (files.clone(), self.sources.clone());
        let work = cx.background_spawn(async move {
            Found {
                sides: sources.map(|side| read(side, &hooks)),
            }
        });
        self.load = Some(cx.spawn(async move |this, cx| {
            let found = work.await;
            let _ = this.update(cx, |view, cx| view.pictures_loaded(found, cx));
        }));
        true
    }

    /// Builds the mask picture for the mask the app sent, when the view can show it.
    fn build_mask(&mut self, cx: &mut Context<NavoWindow>) -> bool {
        let Some(mask) = self.mask.clone() else {
            return false;
        };
        if !self.parts() || self.mask_image.is_some() || self.mask_job.is_some() {
            return false;
        }
        let work = cx.background_spawn(async move { image_body::mask_pixels(&mask) });
        self.mask_job = Some(cx.spawn(async move |this, cx| {
            let pixels = work.await;
            let _ = this.update(cx, |view, cx| view.mask_loaded(pixels, cx));
        }));
        true
    }

    /// Turns what the worker brought back into GPU copies.
    fn upload(&mut self, found: Found, cx: &mut App) {
        let gpu = gpu_images(cx);
        for (index, side) in found.sides.into_iter().enumerate() {
            self.slots[index] = match side {
                Some(Loaded::Picture(picture)) => {
                    let (width, height) = (picture.width, picture.height);
                    Slot::Ready(new_image(picture.pixels, &gpu), width, height)
                }
                Some(Loaded::Failed) => Slot::Failed,
                None => Slot::Idle,
            };
        }
    }

    /// Puts the built mask picture on the GPU.
    fn upload_mask(&mut self, pixels: RgbaImage, cx: &mut App) {
        let gpu = gpu_images(cx);
        self.mask_job = None;
        self.mask_image = Some(new_image(pixels, &gpu));
    }

    /// Gives the pictures a newer result replaced back to the GPU.
    pub(super) fn sweep(&mut self, cx: &mut App) {
        for image in self.garbage.drain(..) {
            release_image(image, cx);
        }
    }

    /// Frees the GPU copies and stops the jobs; the screen calls this before dropping a body.
    pub(super) fn free(&mut self, cx: &mut App) {
        self.sweep(cx);
        self.load = None;
        self.mask_job = None;
        for slot in std::mem::replace(&mut self.slots, [Slot::Idle, Slot::Idle]) {
            if let Slot::Ready(image, _, _) = slot {
                release_image(image, cx);
            }
        }
        if let Some(mask) = self.mask_image.take() {
            release_image(mask, cx);
        }
    }
}

/// Reads and decodes one side off the UI thread; `None` for a side the pair does not have.
fn read(side: (Option<Location>, String), hooks: &FileHooks) -> Option<Loaded> {
    let (location, name) = side;
    let Ok(bytes) = (hooks.bytes)(&location?, IMAGE_LIMIT) else {
        return Some(Loaded::Failed);
    };
    Some(match decode_image(&bytes, &name) {
        Ok(picture) => Loaded::Picture(picture),
        Err(_) => Loaded::Failed,
    })
}
