//! The compare tab's Large wiring (H5): the 1,000 ms start timer, the snapshot after each drain
//! and the exits from Large back to Folders.

use std::time::Duration;

use gpui_kit::{AsyncApp, WeakEntity};
use sidewise_kit::prelude::*;

use super::compare_tab::{CompareTab, TabView};
use super::large_view::{LargeEvent, LargeSnapshot};

/// How long a compare may run before the tab starts on Large instead of Folders.
const SLOW_AFTER: Duration = Duration::from_millis(1000);

/// Waits `SLOW_AFTER`, then asks the tab to start on Large if it is still checking.
pub(super) async fn large_timer(this: WeakEntity<CompareTab>, cx: &mut AsyncApp) {
    cx.background_executor().timer(SLOW_AFTER).await;
    // A failed update means the tab closed.
    let _ = this.update(cx, |tab, cx| tab.start_on_large(cx));
}

impl CompareTab {
    /// Shows Large when the compare still checks and the tab still shows Folders (user keys win).
    fn start_on_large(&mut self, cx: &mut Context<Self>) {
        let checking = self.run.session().is_some_and(|session| session.checking());
        if checking && self.view == TabView::Folders {
            self.view = TabView::Large;
            self.auto_large = true;
            self.focus_pending = true;
            cx.notify();
        }
    }

    /// Hands the Large view the newest meters, totals and slow files. When the session has
    /// finished, a Large that the timer opened goes to Folders; one the user chose with `5` stays.
    pub(super) fn sync_large(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.run.session() else {
            return;
        };
        let snapshot = LargeSnapshot {
            meters: self.run.meters().clone(),
            totals: session.totals(),
            slow: session.slow_paths().to_vec(),
            reading: session.reading().cloned(),
            checking: session.checking(),
        };
        let finished = !snapshot.checking;
        self.large
            .update(cx, |view, cx| view.set_snapshot(snapshot, cx));
        if finished && self.auto_large && self.view == TabView::Large {
            self.auto_large = false;
            self.view = TabView::Folders;
            self.focus_pending = true;
        }
    }

    /// Enter on Large shows Folders.
    pub(super) fn on_large_event(
        &mut self,
        event: &LargeEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let LargeEvent::ShowFolders = event;
        self.open_folders(window, cx);
    }
}
