//! The compare screen entity owns its screens and receives plain inputs from its window.

use std::cell::Cell;
use std::ops::ControlFlow;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::{EventEmitter, Task, canvas};

use super::files::CompareFiles;
use super::look::{Look, LookDefaults};
use super::resync::ResyncStep;
use super::{DiffSlot, ScreenStack};
use crate::compare_diff::{CompareOptions, PairRequest, StreamList};
use crate::prelude::*;

/// The file diff, Stream and inboxes owned by one compare screen.
#[doc(hidden)]
pub struct ScreenParts {
    #[doc(hidden)]
    pub screen: Option<ScreenStack>,
    #[doc(hidden)]
    pub diff: DiffSlot,
}

/// The compare screen and the inputs its window supplies (NC54).
pub struct CompareScreen {
    #[doc(hidden)]
    pub compare: ScreenParts,
    options: CompareOptions,
    files: CompareFiles,
    sides: SharedString,
    stamp: Option<(u64, u64)>,
    look: LookDefaults,
    /// Where the screen was last painted, in window px; the keyboard menu anchors inside it.
    pub(super) painted: Rc<Cell<Bounds<Pixels>>>,
}

impl CompareScreen {
    /// Creates an empty screen with its diff inbox, compare options and file hooks.
    /// Takes any `Into<CompareFiles>`, so Navo can pass its `FileHooks`.
    pub fn new(diff: DiffSlot, options: CompareOptions, files: impl Into<CompareFiles>) -> Self {
        Self {
            compare: ScreenParts { screen: None, diff },
            options,
            files: files.into(),
            sides: SharedString::default(),
            stamp: None,
            look: LookDefaults::default(),
            painted: Rc::default(),
        }
    }

    /// Replaces the folder label and rows stamp before an open or poll.
    pub fn set_inputs(&mut self, sides: SharedString, stamp: Option<(u64, u64)>) {
        self.sides = sides;
        self.stamp = stamp;
    }

    /// Replaces the compare options after the window applies settings.
    pub fn set_options(&mut self, options: CompareOptions) {
        self.options = options;
    }

    /// Stores the host's look defaults and applies them to the open file diff and Stream.
    pub fn set_look(&mut self, look: LookDefaults) {
        self.look = look;
        let applied = Look::from_defaults(look);
        if let Some(screen) = self.compare.screen.as_mut() {
            if let Some(file) = screen.file.as_mut() {
                file.look = applied;
            }
            if let Some(stream) = screen.stream.as_mut() {
                stream.look = applied;
            }
        }
    }

    /// The look defaults new file diffs and Streams start from.
    pub fn look_defaults(&self) -> LookDefaults {
        self.look
    }

    /// The compare options used when opening a pair or Stream.
    pub fn screen_options(&self) -> CompareOptions {
        self.options.clone()
    }

    /// The host's file source, used to read a side's bytes and bodies.
    pub fn screen_files(&self) -> CompareFiles {
        self.files.clone()
    }

    /// The supplied compare roots' names, or empty outside a compare.
    pub fn screen_sides(&self) -> SharedString {
        self.sides.clone()
    }

    /// The supplied rows stamp, or `None` outside a compare.
    pub fn screen_stamp(&self) -> Option<(u64, u64)> {
        self.stamp
    }

    /// Whether a file diff is open in place of the panes.
    pub fn is_file_open(&self) -> bool {
        self.compare
            .screen
            .as_ref()
            .is_some_and(|screen| screen.file.is_some())
    }

    /// Whether the Stream is open in place of the panes.
    pub fn is_stream_open(&self) -> bool {
        self.compare
            .screen
            .as_ref()
            .is_some_and(|screen| screen.stream.is_some())
    }

    /// Returns no work, a wait, or the rebuild's freshness from the Stream's resync decision.
    #[doc(hidden)]
    pub fn resync_step(
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
    #[doc(hidden)]
    pub fn apply_resync_items(
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
    #[doc(hidden)]
    pub fn needs_resync_wait(&self) -> bool {
        self.compare
            .screen
            .as_ref()
            .and_then(|screen| screen.stream.as_ref())
            .is_some_and(|stream| stream.resync_wait.is_none())
    }

    /// Keeps the host's resync timer only while a Stream needs one; otherwise cancels it.
    #[doc(hidden)]
    pub fn set_resync_wait(&mut self, wait: Task<()>) {
        if let Some(stream) = self.stream_mut()
            && stream.resync_wait.is_none()
        {
            stream.resync_wait = Some(wait);
        }
    }

    /// Releases the completed resync task without cancelling its running callback.
    #[doc(hidden)]
    pub fn finish_resync_wait(&mut self) {
        if let Some(wait) = self.stream_mut().and_then(|s| s.resync_wait.take()) {
            wait.detach();
        }
    }

    /// Sends one screen event for the window's subscription to handle.
    #[doc(hidden)]
    pub fn emit_screen(
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
    /// Draws the file diff or Stream with its open context menu (a window overlay that takes
    /// no layout space), or an empty element while neither screen is open.
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let menu = self.compare_menu_view();
        let Some(view) = self.compare_screen_view(window, cx) else {
            return div();
        };
        // The menu sits outside the screens' key contexts: keys it leaves never reach the diff.
        let painted = Rc::clone(&self.painted);
        div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(
                canvas(move |bounds, _, _| painted.set(bounds), |_, (), _, _| {})
                    .absolute()
                    .size_full(),
            )
            .child(view)
            .children(menu)
    }
}

/// A setting the compare screen can ask its window to open, without naming the app's own
/// settings fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingKey {
    /// The memory budget of the large compare, in MiB.
    CompareBudgetMib,
}

/// What the compare screen asks of its window (NC54).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenEvent {
    /// The screen closed; reveal and select this path in the compare pane.
    Closed { reveal: Option<String> },
    /// Settings should open with this field selected.
    OpenSettings(SettingKey),
    /// Open the next (`step` 1) or previous (−1) file of the folder compare after row `from`.
    WalkFile { from: u64, step: i32 },
    /// The resync wait ended: rebuild the Stream from the folder compare.
    Resync,
    /// All Viewed marks were cleared (or could not be): show `text` in a toast; with `cleared`
    /// set, other screens drop their marks too.
    ViewedCleared { text: String, cleared: bool },
    /// A copy or an undo finished: show `text` in a toast, with Undo of copy `job` when set;
    /// other screens reload the `reload` files, which an undo may have rewritten (NC148).
    Wrote {
        text: String,
        job: Option<String>,
        reload: Vec<navo_core::Location>,
    },
}
