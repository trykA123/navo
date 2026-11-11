//! The full-window compare screen (D23): the file diff stacked over the Stream, drawn where the
//! viewer draws. This file holds its state: how a file diff opens, takes updates and pops.

mod context;
mod copy;
mod details;
mod drawers;
mod entry;
mod file;
mod hex;
mod hex_open;
mod host;
pub(in crate::window_view) mod image;
mod inbox;
pub(in crate::window_view) mod large;
pub(in crate::window_view) mod look;
pub(in crate::window_view) mod menu;
mod outline;
mod pick;
mod search;
mod sheet;
mod stream;

use std::sync::{Arc, PoisonError};

use gpui_kit::{FocusHandle, Subscription};

use super::NavoWindow;
use crate::browser::FileDiffRequest;
use crate::compare_diff::{DiffUpdate, Layout, PairControl, PairKind};
use crate::compare_diff::{PairRequest, PairText};
use crate::context_menu::ContextMenu;
use crate::prelude::*;
pub(in crate::window_view) use inbox::DiffSlot;
#[cfg(test)]
pub(in crate::window_view) use inbox::coalesce;
use large::LargeShown;
use stream::StreamScreen;

/// What the file diff shows now.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum FileState {
    Loading,
    Opened {
        kind: PairKind,
        text: Option<Arc<PairText>>,
    },
    Failed(SharedString),
}

/// The worker request id an update belongs to.
fn request_of(update: &DiffUpdate) -> u64 {
    match update {
        DiffUpdate::Opened { request, .. }
        | DiffUpdate::Spans { request, .. }
        | DiffUpdate::Large { request, .. }
        | DiffUpdate::LargeLines { request, .. }
        | DiffUpdate::Bytes { request, .. }
        | DiffUpdate::Image { request, .. }
        | DiffUpdate::ChangedOnDisk { request }
        | DiffUpdate::Failed { request, .. } => *request,
    }
}

/// One open file pair: its request, its control and what arrived for it.
pub(super) struct FileScreen {
    tag: u64,
    request: PairRequest,
    /// The worker request id of the newest update; older ones are stale.
    active: u64,
    /// The newest request id this screen asked for; an `Opened` below it is stale. `None` once
    /// ids ran out, so every update is dropped.
    expected: Option<u64>,
    sides: SharedString,
    focus: FocusHandle,
    state: FileState,
    /// The rows of an opened text pair.
    view: Option<file::FileView>,
    /// The columns of an opened binary pair.
    hex: Option<hex::HexView>,
    /// The change to show first once the text arrives.
    at: Option<usize>,
    /// The large text snapshot applied last; its number was acknowledged after applying.
    large: Option<LargeShown>,
    /// The pair's two pictures, once it opened as an image pair (V40).
    pictures: Option<image::Pictures>,
    /// Image bodies a refresh replaced, until the next poll frees their GPU pictures.
    retired: Vec<image::Pictures>,
    control: Box<dyn PairControl>,
    /// What the View menu set: parts shown and layout; `auto` is the window's layout.
    look: look::Look,
    auto: Layout,
    drawers: drawers::Drawers,
    menu: Option<(Entity<ContextMenu>, Subscription)>,
    /// The lines picked for a partial copy (V32).
    pick: pick::Pick,
}

impl FileScreen {
    /// The byte sizes of a large pair's two sides.
    fn large_sizes(&self) -> Option<(u64, u64)> {
        match &self.state {
            FileState::Opened {
                kind: PairKind::Large { left, right },
                ..
            } => Some((*left, *right)),
            _ => None,
        }
    }

    /// Applies one update, drawing rows `row_height` tall; false when nothing shown changes or
    /// the update is stale.
    fn apply(&mut self, update: DiffUpdate, row_height: Pixels) -> bool {
        let opened = matches!(update, DiffUpdate::Opened { .. });
        let request = request_of(&update);
        let Some(expected) = self.expected else {
            return false;
        };
        if request < expected || (!opened && request != self.active) {
            return false;
        }
        match update {
            DiffUpdate::Opened {
                request,
                kind,
                text,
            } => {
                self.active = request;
                self.expected = Some(request);
                self.pick.clear();
                self.opened((kind, text), row_height);
            }
            DiffUpdate::Failed { text, .. } => {
                (self.view, self.large, self.hex) = (None, None, None);
                self.fail(text);
            }
            DiffUpdate::Spans { spans, .. } => match self.view.as_mut() {
                Some(view) => view.add_spans(spans),
                None => return false,
            },
            DiffUpdate::Large { .. } | DiffUpdate::LargeLines { .. } => {
                self.apply_large(update, row_height);
            }
            DiffUpdate::ChangedOnDisk { .. } => {
                self.refresh();
                return true;
            }
            DiffUpdate::Image { scores, mask, .. } => return self.apply_image(scores, mask),
            DiffUpdate::Bytes {
                align,
                run,
                regions,
                ..
            } => return self.apply_bytes(regions, (align, run)),
        }
        true
    }

    /// Applies a byte snapshot to the hex body; false when the body is gone or stale.
    fn apply_bytes(
        &mut self,
        regions: Vec<crate::compare_diff::ByteRegionView>,
        (align, run): (crate::compare_diff::HexAlign, u64),
    ) -> bool {
        match self.hex.as_mut() {
            Some(hex) => hex.add(regions, (align, run)),
            None => false,
        }
    }

    /// Installs an opened pair: its text rows, its large or image or hex body by kind, or its
    /// kind only; rows are `row_height` tall.
    fn opened(&mut self, (kind, text): (PairKind, Option<Arc<PairText>>), row_height: Pixels) {
        let (at, words) = (self.at, self.look.words);
        self.view = text
            .clone()
            .map(|text| file::FileView::new(text, at, words));
        let layout = self.look.layout.unwrap_or(self.auto);
        self.large =
            matches!(kind, PairKind::Large { .. }).then(|| LargeShown::new(row_height, layout));
        let old = self.hex.take();
        self.hex = matches!(kind, PairKind::Binary).then(|| hex::HexView::reopened(old));
        self.open_pictures(kind);
        self.state = FileState::Opened { kind, text };
        self.follow_layout();
    }

    /// Asks the worker to reload both sides under the next request id; updates for every earlier
    /// id become stale. Fails the pair, without reusing an id, once the ids run out.
    fn refresh(&mut self) {
        let next = self.expected.and_then(|id| id.checked_add(1));
        (self.expected, self.large) = (next, None);
        if let Some(id) = next {
            self.active = id;
        }
        match next {
            Some(id) => self.control.refresh(id),
            None => {
                self.view = None;
                self.state = FileState::Failed(navo_l10n::tr("compare-could-not-start").into());
            }
        }
    }
}

/// The screen in place of the panes: a Stream, a file diff on top of it, or both.
#[derive(Default)]
pub(super) struct CompareScreen {
    stream: Option<StreamScreen>,
    file: Option<FileScreen>,
}

/// Drops a closed file screen; its texts' last references go to a background task, so a
/// large pair is freed off the UI thread.
fn release(file: Option<FileScreen>, cx: &mut App) {
    let Some(mut file) = file else {
        return;
    };
    let mut texts: Vec<Arc<PairText>> = file
        .view
        .as_ref()
        .map(|v| Arc::clone(v.text()))
        .into_iter()
        .collect();
    if let FileState::Opened {
        text: Some(text), ..
    } = &file.state
    {
        texts.push(Arc::clone(text));
    }
    file.free_pictures(cx);
    drop(file);
    cx.background_executor()
        .spawn(async move { drop(texts) })
        .detach();
}

/// The compare roots' names as "left ↔ right", or empty outside a compare.
fn sides_label(window: &NavoWindow) -> SharedString {
    let Some(mode) = window.browser.compare_mode() else {
        return SharedString::default();
    };
    let [left, right] = mode.roots().clone().map(|root| {
        root.name()
            .map_or_else(|| root.as_str().to_owned(), |name| name.to_text())
    });
    navo_l10n::t!("compare-screen-sides", left = left, right = right).into()
}

impl NavoWindow {
    /// Opens the file diff Enter asked for; false when none was asked or the app has no hooks.
    pub(super) fn open_file_diff(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(asked) = self.browser.take_file_diff() else {
            return false;
        };
        let FileDiffRequest {
            id,
            path,
            left,
            right,
        } = asked;
        let request = PairRequest {
            id,
            path,
            left,
            right,
        };
        self.show_pair((request, None), window, cx)
    }

    /// Opens `request` in the file diff, over any Stream, replacing an open file diff and
    /// focusing it; `at` is the change to show first. False when the app has no diff hooks.
    fn show_pair(
        &mut self,
        (request, at): (PairRequest, Option<usize>),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(hooks) = self.compare.diff.hooks.clone() else {
            return false;
        };
        self.compare.diff.opened += 1;
        let tag = self.compare.diff.opened;
        let control = hooks.open_pair(request.clone(), &self.values, self.compare.diff.sink(tag));
        let focus = cx.focus_handle();
        let screen = self.compare.screen.as_ref();
        let look = screen
            .and_then(|s| s.file.as_ref().map(|file| file.look))
            .or_else(|| {
                screen
                    .and_then(|s| s.stream.as_ref())
                    .map(|stream| stream.look)
            })
            .unwrap_or_else(look::Look::rich);
        let active = request.id;
        let file = FileScreen {
            tag,
            request,
            active,
            expected: Some(active),
            sides: sides_label(self),
            focus: focus.clone(),
            state: FileState::Loading,
            view: None,
            hex: None,
            at,
            large: None,
            pictures: None,
            retired: Vec::new(),
            control,
            look,
            auto: file::layout_for(window),
            drawers: drawers::Drawers::new(),
            menu: None,
            pick: pick::Pick::default(),
        };
        let old = self
            .compare
            .screen
            .get_or_insert_with(CompareScreen::default)
            .file
            .replace(file);
        release(old, cx);
        window.defer(cx, move |window, cx| window.focus(&focus, cx));
        cx.notify();
        true
    }

    /// Drains the inboxes of the Stream and the file diff; updates for a closed or replaced
    /// file screen are dropped. Returns whether the screen changed.
    pub(super) fn poll_compare_screen(&mut self, cx: &mut Context<Self>) -> bool {
        let rows = crate::tokens::current(cx).spacing.row_height;
        self.poll_stream(cx)
            | self.poll_file_screen(rows, cx)
            | self.poll_copies()
            | self.poll_sheet()
            | self.load_pictures(cx)
    }

    /// Drains the file diff's inbox into rows `rows` tall, then starts a binary body's open.
    /// Returns whether the screen changed.
    fn poll_file_screen(&mut self, rows: Pixels, cx: &mut Context<Self>) -> bool {
        let updates = std::mem::take(
            &mut *self
                .compare
                .diff
                .inbox
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
        let Some(file) = self.compare.screen.as_mut().and_then(|s| s.file.as_mut()) else {
            return false;
        };
        let tag = file.tag;
        let changed = updates
            .into_iter()
            .filter(|(sent_to, _)| *sent_to == tag)
            .fold(false, |changed, (_, update)| {
                file.apply(update, rows) | changed
            });
        self.start_hex_open(cx);
        changed
    }

    /// Esc on the file diff: pops it and cancels its pair. Focus goes to the Stream underneath,
    /// or with none, back to the compare pane with the shown file's row revealed and selected.
    fn pop_file_diff(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(screen) = self.compare.screen.as_mut() else {
            return;
        };
        if screen
            .file
            .as_mut()
            .is_some_and(|file| file.drawers.close_one())
        {
            cx.notify();
            return;
        }
        let file = screen.file.take();
        let path = file.as_ref().map(|file| file.request.path.clone());
        if let (Some(file), Some(stream)) = (&file, screen.stream.as_mut()) {
            stream.look = file.look;
        }
        release(file, cx);
        self.compare.diff.copy.note = None;
        match &screen.stream {
            Some(stream) => window.focus(&stream.focus, cx),
            None => {
                self.compare.screen = None;
                if let Some(path) = path {
                    self.browser.compare_reveal(&path);
                }
                self.focus_core_pane(window, cx);
            }
        }
        cx.notify();
    }
}

#[cfg(test)]
mod probes;
#[cfg(test)]
pub(in crate::window_view) use hex::HexProbe;
