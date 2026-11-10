//! When the Stream asks the app to search again after its item list changed (V29): at once for
//! a new generation, at most once per second for the same one, with a timer for the last batch.

use std::time::{Duration, Instant};

use gpui_kit::Task;

use crate::window_view::compare_screen::stream::StreamScreen;

/// The shortest time between two reruns under the same generation.
const RERUN: Duration = Duration::from_secs(1);

/// The throttle of reruns: when the last one ran, and whether a later batch still waits.
#[derive(Default)]
pub(in crate::window_view::compare_screen) struct Rerun {
    last: Option<Instant>,
    pending: bool,
    /// The timer that runs the waiting rerun.
    pub(in crate::window_view::compare_screen) wait: Option<Task<()>>,
}

impl StreamScreen {
    /// Searches again now when `fresh` (a new generation) or when a second has passed since the
    /// last rerun; else marks the rerun as waiting.
    pub(in crate::window_view::compare_screen) fn search_rerun_soon(&mut self, fresh: bool) {
        let ready = self.rerun.last.is_none_or(|at| at.elapsed() >= RERUN);
        if fresh || ready {
            self.search_rerun_now();
        } else {
            self.rerun.pending = true;
        }
    }

    /// Searches again at once and forgets any waiting rerun.
    fn search_rerun_now(&mut self) {
        self.rerun.pending = false;
        if self.search_rerun() {
            self.rerun.last = Some(Instant::now());
        }
    }

    /// How long until the waiting rerun may run; `None` when none waits or a timer is set.
    pub(in crate::window_view::compare_screen) fn rerun_wait(&self) -> Option<Duration> {
        if !self.rerun.pending || self.rerun.wait.is_some() {
            return None;
        }
        let last = self.rerun.last?;
        Some(RERUN.saturating_sub(last.elapsed()))
    }

    /// The timer ended: runs the waiting rerun, if no other rerun took its place.
    pub(in crate::window_view::compare_screen) fn rerun_due(&mut self) {
        if let Some(wait) = self.rerun.wait.take() {
            // Detached, not dropped: this runs inside that task.
            wait.detach();
        }
        if self.rerun.pending {
            self.search_rerun_now();
        }
    }
}
