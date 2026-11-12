//! The compare screen entity owns its screens and receives plain inputs from its window.

use std::ops::ControlFlow;
use std::time::{Duration, Instant};

use gpui_kit::{EventEmitter, Task};

use super::resync::ResyncStep;
use super::{DiffSlot, ScreenStack};
use crate::compare_diff::{CompareOptions, PairRequest, StreamList};
use crate::prelude::*;
use crate::settings::Field;
use crate::window_view::FileHooks;

/// The file diff, Stream and inboxes owned by one compare screen.
pub(in crate::window_view) struct ScreenParts {
    pub(in crate::window_view) screen: Option<ScreenStack>,
    pub(in crate::window_view) diff: DiffSlot,
}

/// The compare screen and the inputs its window supplies (NC54).
pub(in crate::window_view) struct CompareScreen {
    pub(in crate::window_view) compare: super::ScreenParts,
    options: CompareOptions,
    files: FileHooks,
    sides: SharedString,
    stamp: Option<(u64, u64)>,
}

impl CompareScreen {
    /// Creates an empty screen with its diff inbox, compare options and file hooks.
    pub(in crate::window_view) fn new(
        diff: DiffSlot,
        options: CompareOptions,
        files: FileHooks,
    ) -> Self {
        Self {
            compare: ScreenParts { screen: None, diff },
            options,
            files,
            sides: SharedString::default(),
            stamp: None,
        }
    }

    /// Replaces the folder label and rows stamp before an open or poll.
    pub(in crate::window_view) fn set_inputs(
        &mut self,
        sides: SharedString,
        stamp: Option<(u64, u64)>,
    ) {
        self.sides = sides;
        self.stamp = stamp;
    }

    /// Replaces the compare options after the window applies settings.
    pub(in crate::window_view) fn set_options(&mut self, options: CompareOptions) {
        self.options = options;
    }

    /// The compare options used when opening a pair or Stream.
    pub(in crate::window_view) fn screen_options(&self) -> CompareOptions {
        self.options.clone()
    }

    /// The app's file hooks, used to read a side's bytes and bodies.
    pub(in crate::window_view) fn screen_files(&self) -> FileHooks {
        self.files.clone()
    }

    /// The supplied compare roots' names, or empty outside a compare.
    pub(in crate::window_view) fn screen_sides(&self) -> SharedString {
        self.sides.clone()
    }

    /// The supplied rows stamp, or `None` outside a compare.
    pub(in crate::window_view) fn screen_stamp(&self) -> Option<(u64, u64)> {
        self.stamp
    }

    /// Returns no work, a wait, or the rebuild's freshness from the Stream's resync decision.
    pub(in crate::window_view) fn resync_step(
        &self,
        stamp: (u64, u64),
        checking: bool,
        now: Instant,
    ) -> Option<ControlFlow<Duration, bool>> {
        let stream = self.compare.screen.as_ref()?.stream.as_ref()?;
        match stream.resync_step(stamp, checking, now) {
            ResyncStep::Skip => None,
            ResyncStep::Wait(wait) => Some(ControlFlow::Break(wait)),
            ResyncStep::Rebuild { fresh } => Some(ControlFlow::Continue(fresh)),
        }
    }

    /// Applies the host-built items with their stamp, executor time and freshness; false if unchanged.
    pub(in crate::window_view) fn apply_resync_items(
        &mut self,
        items: (StreamList, Vec<PairRequest>),
        (stamp, now, fresh): ((u64, u64), Instant, bool),
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(stream) = self.stream_mut() else {
            return false;
        };
        let changed = stream.resync_apply(stamp, now, items, fresh);
        self.arm_rerun(cx);
        changed
    }

    /// Whether the open Stream needs a resync timer and has none attached.
    pub(in crate::window_view) fn needs_resync_wait(&self) -> bool {
        self.compare
            .screen
            .as_ref()
            .and_then(|screen| screen.stream.as_ref())
            .is_some_and(|stream| stream.resync_wait.is_none())
    }

    /// Keeps the host's resync timer only while a Stream needs one; otherwise cancels it.
    pub(in crate::window_view) fn set_resync_wait(&mut self, wait: Task<()>) {
        if let Some(stream) = self.stream_mut()
            && stream.resync_wait.is_none()
        {
            stream.resync_wait = Some(wait);
        }
    }

    /// Releases the completed resync task without cancelling its running callback.
    pub(in crate::window_view) fn finish_resync_wait(&mut self) {
        if let Some(wait) = self.stream_mut().and_then(|s| s.resync_wait.take()) {
            wait.detach();
        }
    }

    /// Sends one screen event for the window's subscription to handle.
    pub(in crate::window_view) fn emit_screen(
        &mut self,
        event: ScreenEvent,
        _window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        cx.emit(event);
    }
}

impl EventEmitter<ScreenEvent> for CompareScreen {}

impl Render for CompareScreen {
    /// Draws the file diff or Stream, or an empty element while neither is open.
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.compare_screen_view(window, cx)
            .unwrap_or_else(|| div().into_any_element())
    }
}

/// What the compare screen asks of its window (NC54).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::window_view) enum ScreenEvent {
    /// The screen closed; reveal and select this path in the compare pane.
    Closed { reveal: Option<String> },
    /// Settings should open with this field selected.
    OpenSettings(Field),
    /// Open the next (`step` 1) or previous (−1) file of the folder compare after row `from`.
    WalkFile { from: u64, step: i32 },
    /// The resync wait ended: rebuild the Stream from the folder compare.
    Resync,
}
