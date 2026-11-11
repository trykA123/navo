//! The image body's seams with the file screen and the window: opening and retiring the body,
//! the status line, the poll that starts its jobs, and the key and pointer entries (V40).

use gpui_kit::{Context, Pixels};

use super::{Found, Pictures, Pointer};
use crate::compare_diff::image_body::ImageMode;
use crate::compare_diff::{ImageMask, ImageScores, PairKind};
use crate::prelude::*;
use crate::window_view::NavoWindow;
use crate::window_view::compare_screen::look::Preset;
use crate::window_view::compare_screen::{FileScreen, FileState};

impl FileScreen {
    /// Starts the image body when the pair opened as an image pair; any earlier body retires.
    pub(in crate::window_view::compare_screen) fn open_pictures(&mut self, kind: PairKind) {
        self.retire_pictures();
        if kind == PairKind::Image {
            let rich = self.look.preset != Preset::Simple;
            self.pictures = Some(Pictures::new(&self.request, rich));
        }
    }

    /// Shows the failure text in place of the pair; the picture body retires.
    pub(in crate::window_view::compare_screen) fn fail(&mut self, text: String) {
        self.retire_pictures();
        self.state = FileState::Failed(text.into());
    }

    /// Sets the body aside; the next poll gives its pictures back to the GPU.
    pub(in crate::window_view::compare_screen) fn retire_pictures(&mut self) {
        self.retired.extend(self.pictures.take());
    }

    /// Frees the open body and every retired one.
    pub(in crate::window_view::compare_screen) fn free_pictures(&mut self, cx: &mut App) {
        self.retire_pictures();
        for mut old in self.retired.drain(..) {
            old.free(cx);
        }
    }

    /// Hands the app's scores and mask to the body; false when nothing shown changes.
    pub(in crate::window_view::compare_screen) fn apply_image(
        &mut self,
        scores: ImageScores,
        mask: Option<ImageMask>,
    ) -> bool {
        let rich = self.look.preset != Preset::Simple;
        let request = &self.request;
        let body = self
            .pictures
            .get_or_insert_with(|| Pictures::new(request, rich));
        body.set_result(scores, mask)
    }
}

impl NavoWindow {
    /// Gives retired pictures back, then starts the body's jobs once its scores allow it.
    /// True only when a job started, so an idle screen is not redrawn on every wake.
    pub(in crate::window_view::compare_screen) fn load_pictures(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let files = self.files.clone();
        let Some(file) = self.compare.screen.as_mut().and_then(|s| s.file.as_mut()) else {
            return false;
        };
        for mut old in file.retired.drain(..) {
            old.free(cx);
        }
        let Some(body) = file.pictures.as_mut() else {
            return false;
        };
        body.sweep(cx);
        body.ensure_load(&files, cx)
    }

    /// The body of the open image pair, or `None` for any other pair.
    #[cfg(test)]
    pub(in crate::window_view) fn pictures(&self) -> Option<&Pictures> {
        self.compare
            .screen
            .as_ref()?
            .file
            .as_ref()?
            .pictures
            .as_ref()
    }

    /// One key of the image body, from the file diff root; true when it moved something.
    pub(in crate::window_view::compare_screen) fn image_key(&mut self, key: &str) -> bool {
        let moved = self.pictures_mut().and_then(|body| body.key(key));
        moved == Some(true)
    }

    /// Follows the pointer over the wipe.
    pub(in crate::window_view) fn image_pointer(
        &mut self,
        at: Pointer,
        x: Pixels,
        cx: &mut Context<Self>,
    ) {
        if self.pictures_mut().is_some_and(|body| body.pointer(at, x)) {
            cx.notify();
        }
    }

    /// Opens the view one picker button chose.
    pub(in crate::window_view::compare_screen) fn image_mode(
        &mut self,
        mode: ImageMode,
        cx: &mut Context<Self>,
    ) {
        if self.pictures_mut().is_some_and(|body| body.set_mode(mode)) {
            cx.notify();
        }
    }

    fn pictures_mut(&mut self) -> Option<&mut Pictures> {
        self.compare
            .screen
            .as_mut()?
            .file
            .as_mut()?
            .pictures
            .as_mut()
    }

    /// Applies one worker's decoded pair.
    pub(super) fn pictures_loaded(&mut self, found: Found, cx: &mut Context<Self>) {
        if let Some(body) = self.pictures_mut() {
            body.upload(found, cx);
        }
        cx.notify();
    }

    /// Applies one worker's built mask picture.
    pub(super) fn mask_loaded(&mut self, pixels: image::RgbaImage, cx: &mut Context<Self>) {
        if let Some(body) = self.pictures_mut() {
            body.upload_mask(pixels, cx);
        }
        cx.notify();
    }
}

/// What the image body shows: the open view, where the wipe is, and what the app said.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::window_view) struct Probe {
    pub(in crate::window_view) mode: ImageMode,
    pub(in crate::window_view) split: f32,
    pub(in crate::window_view) parts: bool,
    pub(in crate::window_view) similarity: Option<u32>,
    pub(in crate::window_view) notice: Option<&'static str>,
    pub(in crate::window_view) mask: bool,
    pub(in crate::window_view) mask_ready: bool,
    pub(in crate::window_view) ready: usize,
    /// The size of the placed pictures and the x of their left edge, once a frame drew them.
    pub(in crate::window_view) placed: Option<(f32, f32, f32)>,
}

#[cfg(test)]
impl NavoWindow {
    /// The image body's state, or `None` when no image pair is open.
    pub(in crate::window_view) fn image_probe(&self) -> Option<Probe> {
        let body = self.pictures()?;
        Some(Probe {
            mode: body.mode(),
            split: body.split(),
            parts: body.parts(),
            similarity: body.model.similarity(),
            notice: body.notice(),
            mask: body.mask.is_some(),
            mask_ready: body.mask_image.is_some(),
            ready: body
                .slots
                .iter()
                .filter(|slot| matches!(slot, super::Slot::Ready(..)))
                .count(),
            placed: body.stage.borrow().map(|b| {
                (
                    f32::from(b.size.width),
                    f32::from(b.size.height),
                    f32::from(b.origin.x),
                )
            }),
        })
    }
}
