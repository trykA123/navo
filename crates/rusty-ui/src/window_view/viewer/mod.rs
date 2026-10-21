mod acts;
mod archive;
mod background;
mod body;
mod cancel;
mod editor;
mod elf;
mod encoding;
mod files;
mod font;
mod image;
mod keys;
mod keys_bar;
mod kinds;
mod lister;
mod map;
mod markdown;
mod mdf;
mod memory;
mod menu;
mod modes;
mod open;
mod paged;
pub(in crate::window_view) mod parts;
mod pdf;
mod rich_header;
mod search;
mod space;
mod table;
mod trace;
mod tree;
mod view;
pub(in crate::window_view) mod view_menu;

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::{Bounds, FocusHandle, Subscription};
use rusty_core::Location;
use rusty_core::encoding::{Encoding, TextEncoding};

pub use archive::{ArchiveSource, ArchiveView};
pub use body::{Attach, BodyAct, BodyKeys, Carried, Host, Key, MenuEntry, ViewerBody};
pub use encoding::encoding_picker;
pub use files::{
    FileHooks, FileStamp, FileText, Head, IndexRun, Paged, ReadBytes, Reopen, Save, SaveError,
    Saved, SharedSource,
};
pub use font::{FontFace, FontView};
pub use image::{ExifEntry, IMAGE_LIMIT, ImageView, Picture, decode_image};
pub use keys::bind_keys;
use keys::*;
pub(in crate::window_view) use keys::{BackspaceKey, KeepViewDefault, PageDown, ResetView};
pub use kinds::{Declined, EDIT_LIMIT, Kind, Kinds, Load, Loaded, Slot, Sniff, ViewerKind};
pub use menu::viewer_menu;
pub use paged::{PagedKind, PagedView};
pub use pdf::{OpenPdf, PdfLook, PdfPages, PdfStart, PdfView};
pub use search::memory::MemoryLink;
pub use space::{FileBytes, FileBytesBar, FilePart, Legend, RegionHead, UsageBar, UsageLine};
pub use space::{
    Item as StripItem, Label as StripLabel, MemoryStrip, Step as StripStep, StripData, StripEvent,
};
pub use space::{Treemap, TreemapEvent, TreemapGroup, TreemapPart};
pub(in crate::window_view) use tree::FilterBack;
pub use view_menu::view_menu;

use super::FerriteWindow;
use crate::prelude::*;
use open::Opening;
pub(super) use open::{Opened, Picked, Request, Turn, sniff_and_load};

pub(in crate::window_view) const CONTEXT: &str = "Viewer";

#[cfg(test)]
pub(in crate::window_view) fn paged_top(body: &dyn ViewerBody, cx: &App) -> Option<u64> {
    paged::PagedBody::of(body).map(|paged| paged.0.read(cx).top)
}
const PAGED_CONTEXT: &str = "Paged";
const IMAGE_CONTEXT: &str = "Image";
pub(super) const LINE_BREAK: &str = if cfg!(windows) { "\r\n" } else { "\n" };
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
    menu: Option<(Entity<crate::context_menu::ContextMenu>, Subscription)>,
    is_view_menu: bool,
    parts: parts::ViewParts,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    start_line: Option<u64>,
    start_note: Option<SharedString>,
    encoding: Option<&'static Encoding>,
    detected: Option<TextEncoding>,
    reencoding: Option<u64>,
}

fn merged_note(load: Option<&'static str>, start: Option<SharedString>) -> Option<SharedString> {
    match (load, start) {
        (Some(load), Some(start)) => Some(format!("{load} {start}").into()),
        (load, start) => start.or(load.map(SharedString::new_static)),
    }
}

fn platform_lines(text: &str) -> String {
    text.replace('\n', LINE_BREAK)
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
            menu: None,
            is_view_menu: false,
            parts: parts::ViewParts::default(),
            bounds: Rc::default(),
            start_line: None,
            start_note: None,
            encoding: None,
            detected: None,
            reencoding: None,
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
        if let Some(location) = self.row_location(row) {
            self.view_location(location, window, cx);
        }
    }

    fn row_location(&self, row: u64) -> Option<Location> {
        let tab = self.browser.state().active_tab();
        tab.listing()
            .and_then(|listing| listing.get(row))
            .filter(|entry| !entry.is_folder)
            .and_then(|entry| {
                tab.location()
                    .join(&rusty_core::ProviderPath::from_text(&entry.name))
                    .ok()
            })
    }

    pub(in crate::window_view) fn view_location(
        &mut self,
        location: Location,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut viewer = Viewer::new(location, cx.focus_handle());
        viewer.opening = Some(self.open_file(&viewer, window, cx));
        self.show_viewer(viewer, window, cx);
    }

    pub(in crate::window_view) fn open_viewer_at_line(
        &mut self,
        line: Option<u64>,
        note: Option<SharedString>,
    ) {
        if let Some(viewer) = &mut self.viewer {
            viewer.start_line = line;
            viewer.start_note = note;
        }
    }

    fn step_to(&mut self, location: Location, window: &mut Window, cx: &mut Context<Self>) {
        let in_flight = self
            .viewer
            .as_mut()
            .and_then(|viewer| viewer.opening.take());
        let Some(mut opening) = in_flight else {
            return self.view_location(location, window, cx);
        };
        opening.supersede();
        let mut viewer = Viewer::new(location, cx.focus_handle());
        viewer.opening = Some(opening);
        self.show_viewer(viewer, window, cx);
    }

    fn show_viewer(&mut self, viewer: Viewer, window: &mut Window, cx: &mut Context<Self>) {
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
        if self.settings.is_open() {
            self.focus_settings(window, cx);
        } else {
            self.focus_core_pane(window, cx);
        }
        cx.notify();
    }
}

#[cfg(test)]
impl FerriteWindow {
    pub(in crate::window_view) fn viewer_location(&self) -> Option<&Location> {
        self.viewer.as_ref().map(|viewer| &viewer.location)
    }
}

#[cfg(test)]
mod tests;
