//! Folder compare as a mode of the two panes (D10): state, events and paired navigation.
//!
//! The app starts the engine through `CompareHooks`; updates arrive on the browser's channel
//! tagged with the run that asked for them, so a left compare never feeds a new one.

use std::sync::Arc;

use navo_core::PaneId;

use super::Browser;
use super::message::Message;
use crate::settings::Values;

mod binding;
mod hooks;
mod mode;
mod navigate;
mod rows;
mod stream;
#[cfg(test)]
mod tests;
mod view;

use binding::{PaneBinding, Saved};
pub use hooks::{
    CompareControl, CompareHooks, CompareKind, CompareRow, CompareSide, CompareSink, CompareTotals,
    CompareUpdate, RowProgress, RowStatus, StopBound,
};
pub use mode::{CompareFilter, CompareMode, FileDiffRequest};
pub use view::{BarView, FILTERS, RowView, SideView, Tone, Total, bar_view, empty_text, pane_rows};
pub(crate) use view::{look_of, status_text};

/// Why a compare did not start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareRefusal {
    NoOtherPane,
    NotAvailable,
}

impl CompareRefusal {
    #[must_use]
    pub fn fluent_id(self) -> &'static str {
        match self {
            Self::NoOtherPane => "compare-needs-two-panes",
            Self::NotAvailable => "compare-could-not-start",
        }
    }
}

#[derive(Default)]
pub(super) struct CompareSlot {
    starter: Option<Arc<dyn CompareHooks>>,
    mode: Option<CompareMode>,
    binding: Option<PaneBinding>,
    runs: u64,
    notice: Option<String>,
    file_diff: Option<FileDiffRequest>,
}

impl Browser {
    pub fn set_compare_hooks(&mut self, hooks: Arc<dyn CompareHooks>) {
        self.hooks.compare.starter = Some(hooks);
    }

    #[must_use]
    pub fn compare_mode(&self) -> Option<&CompareMode> {
        self.hooks.compare.mode.as_ref()
    }

    /// 0 for the left compared pane, 1 for the right one; None outside a compare or for another pane.
    #[must_use]
    pub fn compare_side_of(&self, pane: PaneId) -> Option<usize> {
        self.hooks.compare.binding.as_ref()?.side_of(pane)
    }

    /// For the rows view (U5): selection and filter.
    pub fn compare_mode_mut(&mut self) -> Option<&mut CompareMode> {
        self.hooks.compare.mode.as_mut()
    }

    /// A text to show once, for a compare that failed to start.
    pub fn take_compare_notice(&mut self) -> Option<String> {
        self.hooks.compare.notice.take()
    }

    /// The file pair Enter asked to open (U7 opens the diff).
    pub fn take_file_diff(&mut self) -> Option<FileDiffRequest> {
        self.hooks.compare.file_diff.take()
    }

    /// Whether "compare with the other pane" would start now.
    #[must_use]
    pub fn can_compare(&self) -> bool {
        self.hooks.compare.mode.is_none() && self.compare_plan().is_ok()
    }

    /// The hooks and the two sides a compare would use, or why there is none.
    /// A side the hooks cannot read (archive, FTP, ...) is refused.
    fn compare_plan(&self) -> Result<(Arc<dyn CompareHooks>, [Saved; 2]), CompareRefusal> {
        let hooks = self
            .hooks
            .compare
            .starter
            .clone()
            .ok_or(CompareRefusal::NotAvailable)?;
        let saved = self.compare_sides().ok_or(CompareRefusal::NoOtherPane)?;
        if !saved.iter().all(|side| hooks.supports(&side.location)) {
            return Err(CompareRefusal::NotAvailable);
        }
        Ok((hooks, saved))
    }

    /// Compares the focused pane with the other pane; already comparing does nothing.
    /// The session and its pane binding are set together, and stay that way until a leave.
    pub fn compare_with_other_pane(&mut self, values: &Values) -> Result<(), CompareRefusal> {
        if self.hooks.compare.mode.is_some() {
            return Ok(());
        }
        let (hooks, saved) = self.compare_plan()?;
        self.hooks.compare.runs += 1;
        let run = self.hooks.compare.runs;
        let sink = self.compare_sink(run);
        let sides = (&saved[0].location, &saved[1].location);
        let control = hooks.start(sides, values, sink);
        let focused = self.state.focused_pane();
        let roots = [saved[0].location.clone(), saved[1].location.clone()];
        self.hooks.compare.mode = Some(CompareMode::new(run, control, roots));
        self.hooks.compare.binding = Some(PaneBinding { saved, focused });
        Ok(())
    }

    fn compare_sink(&self, run: u64) -> CompareSink {
        let (sender, wake) = (self.sender.clone(), Arc::clone(&self.wake));
        Arc::new(move |update| {
            if sender.send(Message::Compare(run, update)).is_ok() {
                wake();
            }
        })
    }

    /// The focused pane and its target, left one first by layout order.
    fn compare_sides(&self) -> Option<[Saved; 2]> {
        let window = self.state.window(self.state.focused_window())?;
        let focused = self.state.focused_pane();
        let other = window.target().filter(|&pane| pane != focused)?;
        let numbered = window.layout().numbered();
        let at = |pane| numbered.iter().position(|&p| p == pane);
        let (left, right) = if at(focused)? < at(other)? {
            (focused, other)
        } else {
            (other, focused)
        };
        Some([self.saved_pane(left)?, self.saved_pane(right)?])
    }

    fn saved_pane(&self, pane: PaneId) -> Option<Saved> {
        let tab = self.state.pane(pane)?.active();
        let cursor = tab
            .listing()
            .and_then(|listing| listing.get(tab.view().cursor))
            .map(|entry| entry.name.clone());
        Some(Saved {
            pane,
            location: tab.location().clone(),
            cursor,
        })
    }

    /// Applies an update of the current run; a failed start leaves the mode with a notice.
    pub(super) fn accept_compare(&mut self, run: u64, update: CompareUpdate) -> bool {
        let Some(mode) = self
            .hooks
            .compare
            .mode
            .as_mut()
            .filter(|mode| mode.run == run)
        else {
            return false;
        };
        let changed = mode.apply(update);
        if let Some(text) = mode.failed.take() {
            self.hooks.compare.notice = Some(text);
            self.leave_compare();
        }
        changed
    }
}
