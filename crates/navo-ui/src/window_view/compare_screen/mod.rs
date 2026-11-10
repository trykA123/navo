//! The full-window compare screen (D23): the file diff stacked over the Stream, drawn where the
//! viewer draws. This file holds its state: how a file diff opens, takes updates and pops.

mod drawers;
mod entry;
mod file;
mod host;
pub(in crate::window_view) mod look;
pub(in crate::window_view) mod menu;
mod outline;
mod stream;

use std::sync::{Arc, Mutex, PoisonError};

use gpui_kit::{FocusHandle, Subscription};

use super::NavoWindow;
use crate::browser::{FileDiffRequest, Waker};
use crate::compare_diff::PairRequest;
use crate::compare_diff::{DiffHooks, DiffSink, DiffUpdate, PairControl, PairKind, PairText};
use crate::context_menu::ContextMenu;
use crate::prelude::*;
use stream::StreamScreen;

/// Updates from open pairs, each tagged with the file screen it was sent to.
type Inbox = Arc<Mutex<Vec<(u64, DiffUpdate)>>>;

/// The app's diff hooks and the inbox their sinks fill; one per window.
pub(super) struct DiffSlot {
    hooks: Option<Arc<dyn DiffHooks>>,
    inbox: Inbox,
    wake: Waker,
    opened: u64,
}

impl DiffSlot {
    /// A slot over `hooks` (none until the app wires them) that wakes the window with `wake`.
    pub(super) fn new(hooks: Option<Arc<dyn DiffHooks>>, wake: Waker) -> Self {
        Self {
            hooks,
            inbox: Inbox::default(),
            wake,
            opened: 0,
        }
    }

    /// A sink that tags each update with `tag`, queues it and wakes the window.
    fn sink(&self, tag: u64) -> DiffSink {
        let (inbox, wake) = (Arc::clone(&self.inbox), Arc::clone(&self.wake));
        Arc::new(move |update| {
            inbox
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((tag, update));
            wake();
        })
    }
}

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

/// One open file pair: its request, its control and what arrived for it.
pub(super) struct FileScreen {
    tag: u64,
    request: PairRequest,
    sides: SharedString,
    focus: FocusHandle,
    state: FileState,
    /// The rows of an opened text pair.
    view: Option<file::FileView>,
    /// The change to show first once the text arrives.
    at: Option<usize>,
    control: Box<dyn PairControl>,
    /// What the View menu set: parts shown and layout.
    look: look::Look,
    drawers: drawers::Drawers,
    menu: Option<(Entity<ContextMenu>, Subscription)>,
}

impl FileScreen {
    /// Applies one update; false when nothing shown changes (disk changes come later).
    fn apply(&mut self, update: DiffUpdate) -> bool {
        match update {
            DiffUpdate::Opened { kind, text, .. } => {
                let (at, words) = (self.at, self.look.words);
                self.view = text
                    .clone()
                    .map(|text| file::FileView::new(text, at, words));
                self.state = FileState::Opened { kind, text };
            }
            DiffUpdate::Failed { text, .. } => self.state = FileState::Failed(text.into()),
            DiffUpdate::Spans { spans, .. } => match self.view.as_mut() {
                Some(view) => view.add_spans(spans),
                None => return false,
            },
            DiffUpdate::ChangedOnDisk { .. } => return false,
        }
        true
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
fn release(file: Option<FileScreen>, cx: &App) {
    let Some(file) = file else {
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
        let file = FileScreen {
            tag,
            request,
            sides: sides_label(self),
            focus: focus.clone(),
            state: FileState::Loading,
            view: None,
            at,
            control,
            look,
            drawers: drawers::Drawers::new(),
            menu: None,
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
        self.poll_stream(cx) | self.poll_file_screen()
    }

    /// Drains the file diff's inbox. Returns whether the file screen changed.
    fn poll_file_screen(&mut self) -> bool {
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
        updates
            .into_iter()
            .filter(|(sent_to, _)| *sent_to == tag)
            .fold(false, |changed, (_, update)| file.apply(update) | changed)
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
