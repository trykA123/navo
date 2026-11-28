//! Hex overwrite edit mode for `PagedView`: sparse patch overlay, nibble entry, undo/redo,
//! and background streaming durable save with file-stamp conflict check.

use std::ops::Range;
use std::rc::Rc;
use std::sync::{Arc, PoisonError, RwLock};

use gpui_kit::{KeyDownEvent, Task};
use navo_core::Location;
use navo_fs::{HexFileStamp, HexPatchOverlay, HexSaveError, PatchedByteSource};
use navo_l10n::{t, tr};

use super::PagedView;
use super::lines::PagedKind;
use crate::prelude::*;
use crate::window_view::viewer::{FileHooks, FileStamp, HexSave, HexSaved, SharedSource};

/// Host hooks and location needed to stamp and durably save hex edits.
#[derive(Clone)]
pub(super) struct HexEditLink {
    pub hooks: FileHooks,
    pub location: Location,
    pub on_saved: Rc<dyn Fn(&mut App)>,
    pub is_preview: bool,
}

/// Live hex edit overlay state attached to a `PagedView`.
pub(super) struct HexEditState {
    pub(super) editing: bool,
    pub(super) pending_nibble: Option<(u64, u8)>,
    pub(super) stamp: Option<HexFileStamp>,
    pub(super) fallback_stamp: Option<FileStamp>,
    pub(super) link: Option<HexEditLink>,
    pub(super) saving: Option<Task<()>>,
    pub(super) stamp_job: Option<Task<()>>,
    pub(super) overlay: Arc<RwLock<HexPatchOverlay>>,
    pub(super) base: SharedSource,
}

impl HexEditState {
    pub(super) fn new(
        base: SharedSource,
        overlay: Arc<RwLock<HexPatchOverlay>>,
        fallback_stamp: Option<FileStamp>,
    ) -> Self {
        Self {
            editing: false,
            pending_nibble: None,
            stamp: None,
            fallback_stamp,
            link: None,
            saving: None,
            stamp_job: None,
            overlay,
            base,
        }
    }

    pub(super) fn is_dirty(&self) -> bool {
        self.overlay
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .is_dirty()
    }

    pub(super) fn patched_count(&self) -> usize {
        self.overlay
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .patched_count()
    }

    pub(super) fn revision(&self) -> u64 {
        self.overlay
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .revision()
    }

    pub(super) fn patched_in_range(&self, range: Range<u64>) -> Vec<u64> {
        self.overlay
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .patched_in_range(range)
    }

    pub(super) fn clear_nibble(&mut self) {
        self.pending_nibble = None;
    }
}

fn parse_hex_nibble(key: &str) -> Option<u8> {
    let mut chars = key.chars();
    let digit = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    digit
        .to_digit(16)
        .and_then(|value| u8::try_from(value).ok())
}

impl PagedView {
    /// Connects hex editing to host file hooks and any initial stamp captured on load.
    pub(super) fn attach_hex_edit(
        &mut self,
        link: HexEditLink,
        stamp: Option<HexFileStamp>,
        cx: &mut Context<Self>,
    ) {
        self.hex_edit.link = Some(link);
        if stamp.is_some() {
            self.hex_edit.stamp = stamp;
        } else if self.kind == PagedKind::Hex {
            self.ensure_hex_stamp(cx);
        }
    }

    /// Captures the file's full `HexFileStamp` off the UI thread if not already recorded.
    pub(super) fn ensure_hex_stamp(&mut self, cx: &mut Context<Self>) {
        if self.hex_edit.stamp.is_some() || self.hex_edit.stamp_job.is_some() {
            return;
        }
        let Some(link) = self.hex_edit.link.clone().filter(|link| !link.is_preview) else {
            return;
        };
        let (sender, receiver) = futures::channel::oneshot::channel();
        cx.background_spawn(async move {
            let _ = sender.send((link.hooks.hex_stamp)(&link.location));
        })
        .detach();
        self.hex_edit.stamp_job = Some(cx.spawn(async move |this, cx| {
            let fetched = receiver.await.ok().and_then(Result::ok).flatten();
            let _ = this.update(cx, |view, _| {
                view.hex_edit.stamp_job = None;
                view.hex_edit.stamp = view.hex_edit.stamp.take().or(fetched);
            });
        }));
    }

    /// True when the hex viewer can enter overwrite edit mode (`Ctrl+E`).
    pub(super) fn can_start_hex_edit(&self) -> bool {
        let is_preview = self
            .hex_edit
            .link
            .as_ref()
            .is_some_and(|link| link.is_preview);
        !is_preview
            && self.kind == PagedKind::Hex
            && !self.hex_edit.editing
            && !self.source.is_empty()
    }

    /// Enters hex overwrite edit mode when in hex view; returns true if editing is now active.
    pub(super) fn start_hex_edit(&mut self, cx: &mut Context<Self>) -> bool {
        if self.kind != PagedKind::Hex || self.source.is_empty() {
            return false;
        }
        if self
            .hex_edit
            .link
            .as_ref()
            .is_some_and(|link| link.is_preview)
        {
            return false;
        }
        self.hex_edit.editing = true;
        self.hex_edit.clear_nibble();
        self.ensure_hex_stamp(cx);
        cx.notify();
        true
    }

    /// True while hex overwrite edit mode is active.
    pub(super) fn is_hex_editing(&self) -> bool {
        self.kind == PagedKind::Hex && self.hex_edit.editing
    }

    /// True when the hex patch overlay holds unsaved edits.
    pub(super) fn is_hex_dirty(&self) -> bool {
        self.kind == PagedKind::Hex && self.hex_edit.is_dirty()
    }

    /// Revision of the hex patch overlay for unsaved-change confirmation guards.
    pub(super) fn hex_edit_revision(&self) -> u64 {
        self.hex_edit.revision()
    }

    /// Dirty badge label shown in the hex header when bytes are modified.
    pub(super) fn hex_dirty_label(&self) -> Option<String> {
        if self.kind != PagedKind::Hex {
            return None;
        }
        let count = self.hex_edit.patched_count();
        (count > 0).then(|| t!("hex-edit-modified", count = count))
    }

    /// Handles hex digit entry and Ctrl+Z / Ctrl+Y undo/redo while in hex edit mode.
    pub(super) fn hex_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_hex_editing()
            || self.hex_edit.saving.is_some()
            || !self.focus.is_focused(window)
            || self.find.bar.is_some()
        {
            return;
        }
        let mods = event.keystroke.modifiers;
        let key = event.keystroke.key.as_str();
        let command = (mods.control || mods.platform) && !mods.alt;
        if command && !mods.shift && key.eq_ignore_ascii_case("z") {
            self.hex_undo(cx);
            cx.stop_propagation();
            return;
        }
        if command
            && ((!mods.shift && key.eq_ignore_ascii_case("y"))
                || (mods.shift && key.eq_ignore_ascii_case("z")))
        {
            self.hex_redo(cx);
            cx.stop_propagation();
            return;
        }
        if !mods.control
            && !mods.alt
            && !mods.platform
            && let Some(nibble) = parse_hex_nibble(key)
        {
            self.type_hex_nibble(nibble, cx);
            cx.stop_propagation();
        }
    }

    fn type_hex_nibble(&mut self, nibble: u8, cx: &mut Context<Self>) {
        let offset = self.cursor;
        let mut raw = [0u8; 1];
        let Ok(1) = self.hex_edit.base.read_at(offset, &mut raw) else {
            return;
        };
        let original = raw[0];
        let advance = match self.hex_edit.pending_nibble {
            Some((at, high)) if at == offset => {
                let value = (high << 4) | nibble;
                let mut overlay = self
                    .hex_edit
                    .overlay
                    .write()
                    .unwrap_or_else(PoisonError::into_inner);
                overlay.coalesce_byte(offset, original, value);
                self.hex_edit.pending_nibble = None;
                true
            }
            _ => {
                let mut overlay = self
                    .hex_edit
                    .overlay
                    .write()
                    .unwrap_or_else(PoisonError::into_inner);
                let current = overlay.patched_byte(offset).unwrap_or(original);
                let value = (nibble << 4) | (current & 0x0F);
                overlay.overwrite_byte(offset, original, value);
                self.hex_edit.pending_nibble = Some((offset, nibble));
                false
            }
        };
        self.note = None;
        self.source_rev += 1;
        if advance && offset < self.source.len().saturating_sub(1) {
            self.go_to_byte(offset + 1, cx);
        }
        self.request_rows(cx);
        cx.notify();
    }

    fn hex_undo(&mut self, cx: &mut Context<Self>) {
        self.hex_edit.clear_nibble();
        let target = self
            .hex_edit
            .overlay
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .undo();
        if let Some(offset) = target {
            self.note = None;
            self.source_rev += 1;
            self.go_to_byte(offset, cx);
            self.request_rows(cx);
            cx.notify();
        }
    }

    fn hex_redo(&mut self, cx: &mut Context<Self>) {
        self.hex_edit.clear_nibble();
        let target = self
            .hex_edit
            .overlay
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .redo();
        if let Some(offset) = target {
            self.note = None;
            self.source_rev += 1;
            self.go_to_byte(offset, cx);
            self.request_rows(cx);
            cx.notify();
        }
    }

    /// Streams sparse hex patches to disk in the background with file-stamp conflict checking.
    pub(super) fn save_hex(&mut self, cx: &mut Context<Self>) {
        if !self.is_hex_editing() || self.hex_edit.saving.is_some() {
            return;
        }
        let (patches, revision) = {
            let overlay = self
                .hex_edit
                .overlay
                .read()
                .unwrap_or_else(PoisonError::into_inner);
            if !overlay.is_dirty() {
                return;
            }
            (overlay.snapshot_patches(), overlay.revision())
        };
        let Some(link) = self.hex_edit.link.clone() else {
            return;
        };
        let expected = self.hex_edit.stamp.clone();
        let fallback_stamp = self.hex_edit.fallback_stamp;
        self.note = Some(tr("hex-edit-saving").into());
        let (sender, receiver) = futures::channel::oneshot::channel();
        cx.background_spawn(async move {
            let save = HexSave {
                patches: &patches,
                expected: expected.as_ref(),
                fallback_stamp,
            };
            let result = (link.hooks.write_hex)(&link.location, &save).map(|saved| {
                let refreshed = (link.hooks.paged)(&link.location)
                    .ok()
                    .map(|paged| paged.source);
                (saved, refreshed)
            });
            let _ = sender.send(result);
        })
        .detach();
        self.hex_edit.saving = Some(cx.spawn(async move |this, cx| {
            let outcome = receiver
                .await
                .unwrap_or_else(|_| Err(HexSaveError::Failed(tr("hex-edit-save-failed"))));
            let _ = this.update(cx, |view, cx| view.hex_saved(revision, outcome, cx));
        }));
        cx.notify();
    }

    fn hex_saved(
        &mut self,
        revision: u64,
        outcome: Result<(HexSaved, Option<SharedSource>), HexSaveError>,
        cx: &mut Context<Self>,
    ) {
        self.hex_edit.saving = None;
        match outcome {
            Ok((saved, refreshed)) => self.commit_hex_save(revision, saved, refreshed, cx),
            Err(HexSaveError::FileChangedOnDisk) => {
                self.note = Some(tr("hex-edit-conflict").into());
            }
            Err(error) => {
                self.note = Some(error.to_string().into());
            }
        }
        cx.notify();
    }

    fn commit_hex_save(
        &mut self,
        revision: u64,
        saved: HexSaved,
        refreshed: Option<SharedSource>,
        cx: &mut Context<Self>,
    ) {
        self.hex_edit.stamp = saved.stamp;
        self.hex_edit.fallback_stamp = saved.file_stamp;
        self.hex_edit.clear_nibble();
        if let Some(fresh_base) = refreshed {
            self.hex_edit.base = fresh_base.clone();
            self.source = Arc::new(PatchedByteSource::with_overlay(
                fresh_base,
                self.hex_edit.overlay.clone(),
            ));
        }
        let mut overlay = self
            .hex_edit
            .overlay
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        if overlay.revision() == revision {
            overlay.clear();
        }
        drop(overlay);
        self.changed = false;
        self.source_rev += 1;
        self.note = Some(tr("hex-edit-saved").into());
        if let Some(link) = &self.hex_edit.link {
            (link.on_saved)(cx);
        }
        self.request_rows(cx);
    }
}
