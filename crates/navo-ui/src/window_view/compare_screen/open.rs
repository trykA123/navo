//! The `ScreenOwner` half of the compare screen: how a file diff opens and takes a pair over, how
//! its updates are polled into rows, and how Esc pops it. `release` frees a closed screen's texts
//! on a background thread.

use std::sync::{Arc, PoisonError};

use super::{FileScreen, FileState, ScreenEvent, ScreenOwner, ScreenStack};
use super::{drawers, file, look, pick, switch};
use crate::compare_diff::{CompareOptions, PairRequest, PairText};
use crate::prelude::*;

/// The compare options the owner's settings give the screen.
pub(super) fn opts(window: &ScreenOwner) -> CompareOptions {
    window.screen_options()
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

impl ScreenOwner {
    /// Opens `request` in the file diff, over any Stream, replacing an open file diff and
    /// focusing it; `at` is the change to show first. False when the app has no diff hooks.
    pub(in crate::window_view) fn show_pair(
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
        let control = hooks.open_pair(request.clone(), &opts(self), self.compare.diff.sink(tag));
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
            sides: self.screen_sides(),
            focus: focus.clone(),
            state: FileState::Loading,
            view: None,
            hex: None,
            at,
            switch: switch::Switch::default(),
            large: None,
            carried_sideways: None,
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
            .get_or_insert_with(ScreenStack::default)
            .file
            .replace(file);
        release(old, cx);
        window.defer(cx, move |window, cx| window.focus(&focus, cx));
        cx.notify();
        true
    }

    /// Focuses the open file diff; returns false when none is open.
    pub(in crate::window_view) fn focus_file_screen(
        &self,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let file = self.compare.screen.as_ref().and_then(|s| s.file.as_ref());
        file.map(|file| window.focus(&file.focus, cx)).is_some()
    }

    /// Drains the inboxes of the Stream and the file diff; updates for a closed or replaced
    /// file screen are dropped. `resynced` is whether the host rebuilt the Stream's items this
    /// poll (N4f2). Returns whether the screen changed.
    pub(in crate::window_view) fn poll_compare_screen(
        &mut self,
        resynced: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let rows = crate::tokens::current(cx).spacing.row_height;
        self.poll_stream(resynced, cx)
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
        self.start_file_syntax(cx);
        self.start_file_widths(cx);
        changed
    }

    /// Esc on the file diff: pops it and cancels its pair. Focus goes to the Stream underneath,
    /// or with none, back to the compare pane with the shown file's row revealed and selected.
    pub(in crate::window_view::compare_screen) fn pop_file_diff(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
            Some(stream) => {
                window.focus(&stream.focus, cx);
                cx.notify();
            }
            None => {
                self.compare.screen = None;
                self.emit_screen(ScreenEvent::Closed { reveal: path }, Some(window), cx);
            }
        }
    }
}
