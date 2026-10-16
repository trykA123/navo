mod body;
mod draw;
mod nested;
mod rows;

use std::rc::Rc;

use gpui_kit::{FocusHandle, KeyDownEvent, Task};
use rusty_core::Location;
use rusty_fs::archive::{ArchiveEntry, ArchiveListing, ENCRYPTED, ENTRY_LIMIT};

pub(super) use body::ArchiveBody;

use super::FileHooks;
use super::cancel::Cancel;
use super::paged::PagedBody;
use crate::file_list::typed_char;
use crate::prelude::*;
use crate::primitives::ListScroll;

pub(super) const CONTEXT: &str = "Archive";
const FOLDER_NOTE: &str = "A folder: its files are listed by their full path.";

#[derive(Clone)]
pub struct ArchiveSource {
    pub location: Location,
    pub hooks: FileHooks,
}

struct Wanted {
    index: usize,
    name: String,
}

struct Read {
    name: String,
    result: Result<Vec<u8>, String>,
}

struct Reading {
    name: SharedString,
    _task: Task<()>,
    _cancel: Cancel,
}

struct Totals {
    size: u64,
    has_packed: bool,
}

impl Totals {
    fn of(entries: &[ArchiveEntry]) -> Self {
        Self {
            size: entries
                .iter()
                .fold(0, |total, entry| total.saturating_add(entry.size)),
            has_packed: entries.iter().any(|entry| entry.packed.is_some()),
        }
    }
}

pub struct ArchiveView {
    title: SharedString,
    source: ArchiveSource,
    listing: Rc<ArchiveListing>,
    totals: Totals,
    lowered: Vec<String>,
    shown: Rc<Vec<usize>>,
    filter: String,
    cursor: usize,
    page_rows: usize,
    scroll: ListScroll,
    focus: FocusHandle,
    note: Option<SharedString>,
    reading: Option<Reading>,
    nested: Option<PagedBody>,
}

impl ArchiveView {
    pub fn new(
        title: impl Into<SharedString>,
        source: ArchiveSource,
        listing: ArchiveListing,
        cx: &mut Context<Self>,
    ) -> Self {
        let shown = Rc::new((0..listing.entries.len()).collect());
        let lowered = listing
            .entries
            .iter()
            .map(|entry| entry.name.to_lowercase())
            .collect();
        Self {
            title: title.into(),
            source,
            totals: Totals::of(&listing.entries),
            lowered,
            listing: Rc::new(listing),
            shown,
            filter: String::new(),
            cursor: 0,
            page_rows: 1,
            scroll: ListScroll::new(),
            focus: cx.focus_handle(),
            note: None,
            reading: None,
            nested: None,
        }
    }

    fn entry_at(&self, row: usize) -> Option<(usize, &ArchiveEntry)> {
        let index = *self.shown.get(row)?;
        self.listing.entries.get(index).map(|entry| (index, entry))
    }

    pub(super) fn focus_target(&self, cx: &App) -> FocusHandle {
        use super::ViewerBody as _;
        match &self.nested {
            Some(nested) => nested.focus_handle(cx),
            None => self.focus.clone(),
        }
    }

    pub(super) fn is_typing(&self) -> bool {
        self.nested.is_none() && !self.filter.is_empty()
    }

    fn move_to(&mut self, row: usize, cx: &mut Context<Self>) {
        let last = self.shown.len().saturating_sub(1);
        self.cursor = row.min(last);
        self.scroll.scroll_to(self.cursor);
        cx.notify();
    }

    fn move_by(&mut self, rows: isize, cx: &mut Context<Self>) {
        let row = self.cursor.saturating_add_signed(rows);
        self.move_to(row, cx);
    }

    fn page(&self) -> isize {
        isize::try_from(self.page_rows.max(1)).unwrap_or(1)
    }

    fn refilter(&mut self, cx: &mut Context<Self>) {
        let needle = self.filter.to_lowercase();
        let shown = self
            .lowered
            .iter()
            .enumerate()
            .filter(|(_, name)| name.contains(&needle))
            .map(|(index, _)| index)
            .collect();
        self.shown = Rc::new(shown);
        self.note = None;
        self.move_to(0, cx);
    }

    fn type_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if self.nested.is_some() {
            return;
        }
        if let Some(typed) = typed_char(&event.keystroke) {
            self.filter.push(typed);
        } else if event.keystroke.key == "backspace" && !self.filter.is_empty() {
            self.filter.pop();
        } else {
            return;
        }
        cx.stop_propagation();
        self.refilter(cx);
    }

    fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.nested.take().is_some() {
            window.focus(&self.focus, cx);
            cx.notify();
        } else if self.reading.take().is_some() {
            self.note = None;
            cx.notify();
        } else if !self.filter.is_empty() {
            self.filter.clear();
            self.refilter(cx);
        } else {
            cx.propagate();
        }
    }

    fn click(
        &mut self,
        (row, is_double): (usize, bool),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_to(row, cx);
        if is_double {
            self.open_cursor(window, cx);
        }
    }

    fn open_cursor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.nested.is_some() {
            cx.propagate();
            return;
        }
        let Some((index, entry)) = self
            .entry_at(self.cursor)
            .filter(|_| self.reading.is_none())
        else {
            return;
        };
        let refused = if entry.is_folder {
            Some(FOLDER_NOTE.to_owned())
        } else if entry.is_encrypted {
            Some(ENCRYPTED.to_owned())
        } else if entry.size > ENTRY_LIMIT {
            Some("Over 64 MB: too large to view from the archive.".to_owned())
        } else {
            None
        };
        if let Some(note) = refused {
            self.set_note(note, cx);
            return;
        }
        let name = entry.name.clone();
        self.read_entry(Wanted { index, name }, window, cx);
    }

    fn read_entry(&mut self, wanted: Wanted, window: &mut Window, cx: &mut Context<Self>) {
        let source = self.source.clone();
        let Wanted { index, name } = wanted;
        let shown: SharedString = name.clone().into();
        let cancel = Cancel::new();
        let flag = cancel.flag();
        let task = cx.spawn_in(window, async move |this, cx| {
            let read = async move { (source.hooks.archive_entry)(&source.location, index, &flag) };
            let result = cx.background_executor().spawn(read).await;
            let _ = this.update_in(cx, |view, window, cx| {
                view.entry_read(Read { name, result }, window, cx);
            });
        });
        self.reading = Some(Reading {
            name: shown,
            _task: task,
            _cancel: cancel,
        });
        self.note = None;
        cx.notify();
    }

    fn entry_read(&mut self, read: Read, window: &mut Window, cx: &mut Context<Self>) {
        if self.reading.take().is_none() {
            return;
        }
        match read.result {
            Ok(bytes) => {
                let title = format!("{} › {}", self.title, read.name).into();
                let entry = nested::Entry {
                    title,
                    name: read.name,
                    bytes,
                };
                self.nested = Some(nested::open(entry, cx));
                window.focus(&self.focus_target(cx), cx);
            }
            Err(message) => self.note = Some(message.into()),
        }
        cx.notify();
    }

    pub(super) fn set_note(&mut self, note: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.note = Some(note.into());
        cx.notify();
    }
}

#[cfg(test)]
impl ArchiveView {
    pub(in crate::window_view::viewer) fn nested_kind(&self, cx: &App) -> Option<super::PagedKind> {
        self.nested.as_ref().map(|nested| nested.0.read(cx).kind)
    }
}

impl gpui_kit::Focusable for ArchiveView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
