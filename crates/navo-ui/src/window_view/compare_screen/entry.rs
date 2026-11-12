//! Into the Stream and back (V20): Alt+F opens it over every difference under the compare
//! folder, Enter on a card opens the file diff at the current change, Ctrl+Alt+↓/↑ walk the
//! same files, a change of the compare's rows refreshes the items, and leaving reveals the file.

use std::time::Duration;

use gpui_kit::{Stateful, Task};

use super::stream::StreamScreen;
use crate::browser::{CompareMode, FileDiffRequest};
use crate::commands::compare::{NextFile, OpenFile, PreviousFile};
use crate::compare_diff::{PairRequest, StreamList, is_stream_item, stream_items};
use crate::prelude::*;
use crate::window_view::NavoWindow;

/// The shortest time between two rebuilds of the items while the compare still runs.
const RESYNC: Duration = Duration::from_millis(500);

/// The file diff's form of a compare pair.
fn pair_request(asked: FileDiffRequest) -> PairRequest {
    let FileDiffRequest {
        id,
        path,
        left,
        right,
    } = asked;
    PairRequest {
        id,
        path,
        left,
        right,
    }
}

/// The Stream's items under the compare folder in path order, and each item's pair (D25).
pub(in crate::window_view) fn items_of(mode: &CompareMode) -> (StreamList, Vec<PairRequest>) {
    let list = stream_items(&mode.rows_under_folder(), mode.folder());
    let requests = list
        .items
        .iter()
        .map(|item| {
            mode.pair_of(item.id).map_or_else(
                || PairRequest {
                    id: item.id,
                    path: item.path.clone(),
                    left: None,
                    right: None,
                },
                pair_request,
            )
        })
        .collect();
    (list, requests)
}

/// The pair of the next (`step` 1) or previous (`-1`) Stream item after the row `id`, walking
/// every row under the compare folder once, so a file that is no item still has neighbours.
pub(in crate::window_view) fn neighbour(
    mode: &CompareMode,
    id: u64,
    step: i32,
) -> Option<PairRequest> {
    let found = mode.row_beside(id, step, is_stream_item)?;
    mode.pair_of(found).map(pair_request)
}

impl StreamScreen {
    /// The pair of the current card's file and its current change (the first when none).
    fn current_pair(&self) -> Option<(PairRequest, usize)> {
        let stop = self.model.cursor();
        let item = self.model.items().get(stop.item)?;
        let request = self.requests.get(&item.id)?.clone();
        Some((request, stop.hunk.map_or(0, |hunk| hunk as usize)))
    }

    /// Moves the cursor to the next (`step` 1) or previous (`-1`) file and returns its pair;
    /// `None` at either end.
    fn walk(&mut self, step: i32) -> Option<PairRequest> {
        if !self.model.next_file(step) {
            return None;
        }
        self.sync_rows();
        self.reveal_cursor();
        self.current_pair().map(|(request, _)| request)
    }

    /// The path of the current card's file.
    pub(super) fn current_path(&self) -> Option<String> {
        let item = self.model.items().get(self.model.cursor().item)?;
        Some(item.path.clone())
    }

    /// Replaces the items and their pairs, for the app and the model, under the next generation
    /// when `fresh` (the compare restarted), else under the same one so built cards stay. The
    /// cursor stays on its item while that item is listed. False when nothing changed.
    pub(super) fn replace_items(
        &mut self,
        (list, requests): (StreamList, Vec<PairRequest>),
        fresh: bool,
    ) -> bool {
        if list.items == self.model.items() && list.rules == self.model.rules() {
            return false;
        }
        self.apply_items((list, requests), fresh);
        true
    }

    /// Rebuilds the card of item `id` from its files as they are now, after a copy changed
    /// them; other cards stay. The search runs again, as the card's hunks may have moved.
    pub(super) fn reload_item(&mut self, id: u64) {
        if !self.requests.contains_key(&id) {
            return;
        }
        self.control.reload(id);
        self.search_rerun_soon(true);
        self.sync_rows();
    }

    /// Rebuilds every card from the files as they are now, under the next generation: a sheet
    /// changed many, and the Stream does not know which cards it touched.
    pub(super) fn reload(&mut self) {
        let list = StreamList {
            items: self.model.items().to_vec(),
            rules: self.model.rules(),
        };
        let requests = list
            .items
            .iter()
            .filter_map(|item| self.requests.get(&item.id).cloned())
            .collect();
        self.apply_items((list, requests), true);
    }

    /// Sends `list` and its pairs to the app and the model, under the next generation when
    /// `fresh`, else under the same one.
    fn apply_items(&mut self, (list, requests): (StreamList, Vec<PairRequest>), fresh: bool) {
        let generation = self.model.generation() + u64::from(fresh);
        let settled = list.items.iter().map(|item| item.settled);
        let sent = requests.iter().cloned().zip(settled).collect();
        self.control.set_items(generation, sent);
        self.requests = requests.into_iter().map(|r| (r.id, r)).collect();
        self.model.set_items(generation, list);
        self.search_rerun_soon(fresh);
        self.sync_rows();
    }
}

impl NavoWindow {
    /// Rebuilds the Stream's items when the compare's rows changed since they were built: at
    /// once for a new compare generation or a finished compare, else at most once per 500 ms,
    /// with a timer for a batch that came too soon. Returns whether the items changed.
    pub(super) fn resync_stream(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(mode) = self.browser.compare_mode() else {
            return false;
        };
        let stamp = mode.rows_stamp();
        let now = cx.background_executor().now();
        let Some(stream) = self.compare.screen.as_mut().and_then(|s| s.stream.as_mut()) else {
            return false;
        };
        if stream.stamp == Some(stamp) {
            return false;
        }
        let fresh = stream
            .stamp
            .is_none_or(|(generation, _)| generation != stamp.0);
        let wait = stream.resynced.map_or(Duration::ZERO, |at| {
            (at + RESYNC).saturating_duration_since(now)
        });
        if !fresh && mode.checking() && !wait.is_zero() {
            if stream.resync_wait.is_none() {
                stream.resync_wait = Some(Self::resync_after(wait, cx));
            }
            return false;
        }
        stream.stamp = Some(stamp);
        stream.resynced = Some(now);
        let changed = stream.replace_items(items_of(mode), fresh);
        self.arm_rerun(cx);
        changed
    }

    /// Sets the timer for a search rerun that came too soon, when none is set.
    pub(super) fn arm_rerun(&mut self, cx: &mut Context<Self>) {
        let Some(stream) = self.stream_mut() else {
            return;
        };
        if let Some(wait) = stream.rerun_wait() {
            stream.rerun.wait = Some(Self::rerun_after(wait, cx));
        }
    }

    /// A task that searches again once `wait` has passed.
    fn rerun_after(wait: Duration, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            let _ = this.update(cx, Self::rerun_fired);
        })
    }

    /// The rerun timer ended: searches again and redraws.
    fn rerun_fired(&mut self, cx: &mut Context<Self>) {
        if let Some(stream) = self.stream_mut() {
            stream.rerun_due();
            cx.notify();
        }
    }

    /// A task that rebuilds the items once `wait` has passed.
    fn resync_after(wait: Duration, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            let _ = this.update(cx, Self::resync_due);
        })
    }

    /// The resync wait ended: rebuilds the items and redraws when they changed.
    fn resync_due(&mut self, cx: &mut Context<Self>) {
        // Detached, not dropped: this runs inside that task.
        if let Some(wait) = self.stream_mut().and_then(|s| s.resync_wait.take()) {
            wait.detach();
        }
        if self.resync_stream(cx) {
            cx.notify();
        }
    }

    /// Enter on a card: opens its file in the file diff at the current change.
    fn open_card(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pair = self.stream_ref().and_then(StreamScreen::current_pair);
        if let Some((request, at)) = pair {
            self.show_pair((request, Some(at)), window, cx);
        }
    }

    /// Ctrl+Alt+↓/↑ in the file diff: opens the next or previous file of the Stream underneath,
    /// moving its cursor along, or with no Stream, of the items under the compare folder.
    fn walk_file(&mut self, step: i32, window: &mut Window, cx: &mut Context<Self>) {
        let Some(screen) = self.compare.screen.as_mut() else {
            return;
        };
        let next = match screen.stream.as_mut() {
            Some(stream) => stream.walk(step),
            None => {
                let shown = screen.file.as_ref().map(|file| file.request.id);
                let mode = self.browser.compare_mode();
                mode.zip(shown)
                    .and_then(|(mode, id)| neighbour(mode, id, step))
            }
        };
        if let Some(request) = next {
            self.show_pair((request, Some(0)), window, cx);
        }
    }

    /// The open Stream, under any file diff.
    fn stream_ref(&self) -> Option<&StreamScreen> {
        self.compare.screen.as_ref()?.stream.as_ref()
    }

    /// Binds Ctrl+Alt+↓/↑ (next and previous file) on the file diff's `root`.
    pub(super) fn walk_keys(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &NextFile, window, cx| {
            view.walk_file(1, window, cx);
        }))
        .on_action(cx.listener(|view, _: &PreviousFile, window, cx| {
            view.walk_file(-1, window, cx);
        }))
    }

    /// Binds Enter (open the current card's file) on the Stream's `root`.
    pub(super) fn open_card_key(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &OpenFile, window, cx| {
            view.open_card(window, cx);
        }))
    }
}
