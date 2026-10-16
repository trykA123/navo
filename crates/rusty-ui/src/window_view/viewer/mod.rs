mod acts;
mod archive;
mod background;
mod body;
mod cancel;
mod editor;
mod files;
mod font;
mod image;
mod keys;
mod keys_bar;
mod kinds;
mod lister;
mod markdown;
mod modes;
mod open;
mod paged;
mod rich_header;
mod search;
mod table;
mod tree;
mod view;

use gpui_kit::FocusHandle;
use rusty_core::Location;

pub use archive::{ArchiveSource, ArchiveView};
pub use body::{Attach, BodyAct, BodyKeys, Carried, Host, Key, ViewerBody};
pub use files::{
    FileHooks, FileStamp, FileText, Head, IndexRun, Paged, ReadBytes, Reopen, Save, SaveError,
    SharedSource,
};
pub use font::{FontFace, FontView};
pub use image::{ExifEntry, IMAGE_LIMIT, ImageView, Picture, decode_image};
pub use keys::bind_keys;
use keys::*;
pub use kinds::{Declined, EDIT_LIMIT, Kind, Kinds, Load, Loaded, Slot, Sniff, ViewerKind};
pub use paged::{PagedKind, PagedView};
pub use search::memory::MemoryLink;

use super::FerriteWindow;
use crate::prelude::*;
use open::Opening;

const CONTEXT: &str = "Viewer";
const PAGED_CONTEXT: &str = "Paged";
const IMAGE_CONTEXT: &str = "Image";
const LEAVE_WARNING: &str = "Unsaved changes. Ctrl+S saves; Esc again discards them.";

pub(super) struct Viewer {
    location: Location,
    title: SharedString,
    focus: FocusHandle,
    head: Head,
    kinds: Vec<Kind>,
    kind: Option<Kind>,
    body: Option<Box<dyn ViewerBody>>,
    opening: Option<Opening>,
    leaving: Option<u64>,
    switching: Option<(Slot, u64)>,
    message: Option<SharedString>,
    carried: Option<Carried>,
}

impl Viewer {
    fn new(location: Location, focus: FocusHandle) -> Self {
        let title: SharedString = location
            .name()
            .map_or_else(|| location.as_str().to_owned(), |name| name.to_text())
            .into();
        Self {
            location,
            title,
            focus,
            head: Head::default(),
            kinds: Vec::new(),
            kind: None,
            body: None,
            opening: None,
            leaving: None,
            switching: None,
            message: None,
            carried: None,
        }
    }

    fn sniff(&self) -> Sniff<'_> {
        Sniff {
            head: &self.head.bytes,
            name: &self.title,
            len: self.head.len,
        }
    }

    fn editable_kind(&self) -> Option<Kind> {
        let sniff = self.sniff();
        self.kinds
            .iter()
            .find(|kind| kind.editable(&sniff))
            .cloned()
    }

    fn next_in(&self, slot: Slot) -> Option<Kind> {
        let in_slot: Vec<&Kind> = self
            .kinds
            .iter()
            .filter(|kind| kind.slot() == slot)
            .collect();
        let shown = self.kind.as_ref().map(|kind| kind.id());
        let at = in_slot.iter().position(|kind| Some(kind.id()) == shown);
        let next = at.map_or(0, |at| (at + 1) % in_slot.len());
        in_slot.get(next).map(|kind| Kind::clone(kind))
    }

    fn is_editing(&self, cx: &App) -> bool {
        self.body.as_ref().is_some_and(|body| body.is_editing(cx))
    }

    fn unsaved(&self, cx: &App) -> Option<u64> {
        self.body
            .as_ref()
            .filter(|body| body.is_dirty(cx))
            .map(|body| body.revision(cx))
    }

    fn say(&mut self, note: impl Into<SharedString>, cx: &mut App) {
        let note = note.into();
        match &self.body {
            Some(body) => body.set_note(note, cx),
            None => self.message = Some(note),
        }
    }
}

impl FerriteWindow {
    pub(super) fn view_row(&mut self, row: u64, window: &mut Window, cx: &mut Context<Self>) {
        let tab = self.browser.state().active_tab();
        let location = tab
            .listing()
            .and_then(|listing| listing.get(row))
            .filter(|entry| !entry.is_folder)
            .and_then(|entry| {
                tab.location()
                    .join(&rusty_core::ProviderPath::from_text(&entry.name))
                    .ok()
            });
        if let Some(location) = location {
            self.view_location(location, window, cx);
        }
    }

    pub(super) fn view_location(
        &mut self,
        location: Location,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut viewer = Viewer::new(location, cx.focus_handle());
        viewer.opening = Some(self.open_file(&viewer, window, cx));
        let focus = viewer.focus.clone();
        window.defer(cx, move |window, cx| window.focus(&focus, cx));
        self.viewer = Some(viewer);
        cx.notify();
    }

    pub(super) fn warn_unsaved(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(viewer) = self.viewer.as_mut() else {
            return false;
        };
        let Some(revision) = viewer.unsaved(cx) else {
            return false;
        };
        if viewer.leaving == Some(revision) {
            return false;
        }
        viewer.leaving = Some(revision);
        viewer.say(LEAVE_WARNING, cx);
        cx.notify();
        true
    }

    fn close_viewer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.warn_unsaved(cx) {
            return;
        }
        self.viewer = None;
        self.focus_core_pane(window, cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests;
