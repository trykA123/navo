mod draw;
mod drawers;
mod jobs;
mod keys;
mod text;

#[cfg(test)]
mod tests;

use std::any::Any;
use std::ops::Range;
use std::sync::Arc;

use gpui_kit::{AnyView, FocusHandle, Global, Subscription};
use rusty_automotive::{Format, Image, Item, Layout, ROW_BYTES, Row};

pub(super) use keys::bind_keys;

use super::background::Job;
use super::body::{BodyKeys, Host, ViewerBody};
use super::search::bar::GO_TO_KEYS;
use crate::prelude::*;
use crate::primitives::{ListScroll, TextFieldState};
use crate::side_sheet::SheetState;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::window_view) struct AutomotiveLook {
    pub opens_inspector: bool,
}

impl Global for AutomotiveLook {}

pub(in crate::window_view::viewer) struct MemoryFile {
    pub len: u64,
    pub format: Format,
}

struct Ready {
    image: Arc<Image>,
    layout: Layout,
    items: Vec<Item>,
    wide: bool,
}

enum Index {
    Running { _job: Job, scanned: u64 },
    Ready(Box<Ready>),
    Failed,
}

type Windows = Vec<(Range<u64>, Vec<Option<u8>>)>;

struct Fetched {
    rows: Range<u64>,
    windows: Windows,
}

struct GoTo {
    input: Entity<TextFieldState>,
    error: Option<SharedString>,
    _enter: Subscription,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Drawer {
    Navigator,
    Inspector,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Content,
    Navigator,
}

pub(in crate::window_view::viewer) struct MemoryView {
    title: SharedString,
    len: u64,
    format: Format,
    host: Host,
    focus: FocusHandle,
    scroll: ListScroll,
    nav_scroll: ListScroll,
    index: Index,
    cursor: u64,
    selected: usize,
    focus_in: Focus,
    navigator: Entity<SheetState>,
    inspector: Entity<SheetState>,
    top_drawer: Drawer,
    fetched: Option<Fetched>,
    fetching: Option<(Range<u64>, Job)>,
    go_to: Option<GoTo>,
    note: Option<SharedString>,
    read_error: Option<SharedString>,
    page: u64,
}

impl MemoryView {
    fn new(file: MemoryFile, host: Host, cx: &mut Context<Self>) -> Self {
        let job = jobs::start_index(&host, file.len, file.format, cx);
        Self {
            title: host.title.clone(),
            len: file.len,
            format: file.format,
            host,
            focus: cx.focus_handle(),
            scroll: ListScroll::new(),
            nav_scroll: ListScroll::new(),
            index: Index::Running {
                _job: job,
                scanned: 0,
            },
            cursor: 0,
            selected: 0,
            focus_in: Focus::Content,
            navigator: cx.new(|_| SheetState::new()),
            inspector: cx.new(|_| SheetState::new()),
            top_drawer: Drawer::Navigator,
            fetched: None,
            fetching: None,
            go_to: None,
            note: None,
            read_error: None,
            page: 1,
        }
    }

    fn ready(&self) -> Option<&Ready> {
        match &self.index {
            Index::Ready(ready) => Some(ready),
            Index::Running { .. } | Index::Failed => None,
        }
    }

    fn rows(&self) -> u64 {
        self.ready().map_or(0, |ready| ready.layout.len())
    }

    fn item_of_row(&self, row: u64) -> Option<usize> {
        let ready = self.ready()?;
        match ready.layout.row(row)? {
            Row::Data { address } => ready
                .image
                .item_at(address)
                .or_else(|| ready.image.item_at(address + ROW_BYTES - 1)),
            Row::Gap { range } => ready.image.item_at(range.start),
        }
    }

    fn bytes_at(&self, address: u64) -> Option<&[Option<u8>]> {
        let fetched = self.fetched.as_ref()?;
        let (window, bytes) = fetched
            .windows
            .iter()
            .find(|(window, _)| window.contains(&address))?;
        let at = usize::try_from(address - window.start).ok()?;
        bytes.get(at..at + ROW_BYTES as usize)
    }

    fn set_note(&mut self, note: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.note = Some(note.into());
        cx.notify();
    }
}

pub(in crate::window_view::viewer) struct MemoryBody(pub Entity<MemoryView>);

impl MemoryBody {
    pub(in crate::window_view::viewer) fn new(file: MemoryFile, host: Host, cx: &mut App) -> Self {
        Self(cx.new(|cx| MemoryView::new(file, host, cx)))
    }
}

impl ViewerBody for MemoryBody {
    fn view(&self) -> AnyView {
        self.0.clone().into()
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.0.read(cx).focus.clone()
    }

    fn keys(&self, cx: &App) -> BodyKeys {
        let view = self.0.read(cx);
        if view.go_to.is_some() {
            return BodyKeys::Modal(GO_TO_KEYS.to_vec());
        }
        let mut keys = vec![
            ("Ctrl+B", "Ranges"),
            ("Ctrl+I", "Inspector"),
            ("Ctrl+G", "Go to address"),
        ];
        if view.focus_in == Focus::Navigator {
            keys.insert(0, ("Enter", "Go to range"));
        }
        BodyKeys::Normal(keys)
    }

    fn find_again(&self, _: bool, _: &mut Window, _: &mut App) {}

    fn set_note(&self, note: SharedString, cx: &mut App) {
        self.0.update(cx, |view, cx| view.set_note(note, cx));
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
