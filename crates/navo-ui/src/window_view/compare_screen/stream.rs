//! The Stream screen (V19): every changed file as a card in one gpui `list`. Arriving cards are
//! spliced in so the top visible row stays (D25); J/K, Shift+J/K, V and O move and mark (D27).

mod cards;
mod details;
mod focus;
mod keys;
mod splice;
mod view;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use gpui_kit::{FocusHandle, ListAlignment, ListState, Task};

use super::look::Look;
use super::menu::MenuSlot;
use super::outline::Outline;
use super::search::{Rerun, Search};
use super::{CompareScreen, DiffSlot};
use crate::compare_diff::{
    CardRow, Layout, PairRequest, StreamControl, StreamList, StreamModel, StreamRow, StreamSink,
    StreamUpdate, stream_rows,
};
use crate::prelude::*;
use crate::window_view::NavoWindow;

/// Pixels the list lays out beyond the viewport.
const OVERDRAW: f32 = 200.;
/// Items on each side of the view whose cards the app builds first.
const FOCUS_MARGIN: usize = 10;
/// Items away from the view past which the UI drops a card (D26).
const KEEP_MARGIN: usize = 200;
/// The shortest Stream row (a card line), in px; bounds how many rows fit the viewport.
const MIN_ROW: f32 = 20.;
/// The generation of the first item list; each change of the compare's rows bumps it.
const GENERATION: u64 = 1;

/// Stream updates from the app, waiting for the next poll.
type StreamInbox = Arc<Mutex<Vec<StreamUpdate>>>;

/// The open Stream: its model, the rows the list holds and the app's control.
pub(super) struct StreamScreen {
    pub(super) focus: FocusHandle,
    pub(super) model: StreamModel,
    /// The rows the `ListState` knows, in its order.
    pub(super) rows: Vec<StreamRow>,
    pub(super) list: ListState,
    pub(super) layout: Layout,
    /// What the View menu set; a file diff opened from here starts with it.
    pub(super) look: Look,
    /// The open View menu.
    pub(super) menu: MenuSlot,
    /// Whether the details drawer (Ctrl+I) shows the current file's facts.
    pub(super) details: bool,
    sides: SharedString,
    pub(super) outline: Outline,
    /// The search field, once `/` made it.
    pub(super) search: Option<Search>,
    pub(super) control: Box<dyn StreamControl>,
    /// Each item's pair by item id: Enter and Ctrl+Alt+↓/↑ open it in the file diff.
    pub(super) requests: HashMap<u64, PairRequest>,
    /// The compare's rows stamp the items were built from; `None` outside a compare.
    pub(super) stamp: Option<(u64, u64)>,
    /// When the items were last rebuilt from the compare's rows (executor clock).
    pub(super) resynced: Option<Instant>,
    /// The wait before a rebuild that came too soon after the last one.
    pub(super) resync_wait: Option<Task<()>>,
    /// The throttle of searching again after the items changed.
    pub(super) rerun: Rerun,
    inbox: StreamInbox,
    /// The first and last visible item positions the app last heard as focus.
    sent: Option<(usize, usize)>,
    settle: Option<Task<()>>,
}

/// A sink that queues each update in `inbox` and wakes the window through `slot`.
fn stream_sink(inbox: &StreamInbox, slot: &DiffSlot) -> StreamSink {
    let (inbox, wake) = (Arc::clone(inbox), Arc::clone(&slot.wake));
    Arc::new(move |update| {
        inbox
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(update);
        wake();
    })
}

/// The item index a row belongs to; the rules group has none.
pub(super) fn item_of(row: &StreamRow) -> Option<usize> {
    match *row {
        StreamRow::Header(item)
        | StreamRow::Line { item, .. }
        | StreamRow::Gap { item, .. }
        | StreamRow::More { item, .. }
        | StreamRow::Others { item, .. }
        | StreamRow::Placeholder(item)
        | StreamRow::Summary(item)
        | StreamRow::Detail(item) => Some(item),
        StreamRow::RulesGroup => None,
    }
}

impl StreamScreen {
    /// Rebuilds the rows from the model and splices each changed run into the list, so rows
    /// above the view never move the top visible row.
    pub(super) fn sync_rows(&mut self) {
        let rows = stream_rows(&self.model, self.layout);
        for edit in splice::edits(&self.rows, &rows) {
            self.list.splice(edit.old, edit.new);
        }
        self.rows = rows;
    }

    /// Switches to `layout` (the View menu's, else unified below 900 px) and keeps the current
    /// stop in view.
    fn relayout(&mut self, layout: Layout) {
        if layout != self.layout {
            self.layout = layout;
            self.sync_rows();
            self.reveal_cursor();
        }
    }

    /// The row of the current stop: the first row of its hunk, else its card's header.
    pub(super) fn cursor_row(&self) -> Option<usize> {
        let stop = self.model.cursor();
        let header = self
            .rows
            .iter()
            .position(|row| *row == StreamRow::Header(stop.item))?;
        let card = stop
            .hunk
            .and_then(|_| self.model.card(self.model.items().get(stop.item)?.id));
        let (Some(hunk), Some(card)) = (stop.hunk, card) else {
            return Some(header);
        };
        let in_hunk = |row: &StreamRow| match *row {
            StreamRow::Line { item, card_row, .. } if item == stop.item => matches!(
                card.rows.get(card_row as usize),
                Some(CardRow::Line { hunk: h, .. }) if *h == hunk
            ),
            _ => false,
        };
        let first = self.rows[header..].iter().position(in_hunk);
        Some(first.map_or(header, |at| header + at))
    }

    /// Scrolls the list so the current stop is fully visible.
    pub(super) fn reveal_cursor(&self) {
        if let Some(row) = self.cursor_row() {
            self.list.scroll_to_reveal_item(row);
        }
    }
}

impl NavoWindow {
    /// The open Stream, under any file diff.
    pub(super) fn stream_mut(&mut self) -> Option<&mut StreamScreen> {
        self.compare.screen.as_mut()?.stream.as_mut()
    }

    /// Starts a Stream over `list` through the app's hooks; `requests` locates each item, in
    /// the same order. Returns its focus handle, or `None` when the app has no diff hooks.
    pub(super) fn start_stream(
        &mut self,
        (list, requests): (StreamList, Vec<PairRequest>),
        cx: &mut Context<Self>,
    ) -> Option<FocusHandle> {
        let hooks = self.compare.diff.hooks.clone()?;
        let inbox = StreamInbox::default();
        let control = hooks.open_stream(&self.values, stream_sink(&inbox, &self.compare.diff));
        let settled = list.items.iter().map(|item| item.settled);
        control.set_items(GENERATION, requests.iter().cloned().zip(settled).collect());
        let mut model = StreamModel::default();
        model.set_items(GENERATION, list);
        let rows = stream_rows(&model, Layout::SideBySide);
        let state = ListState::new(rows.len(), ListAlignment::Top, px(OVERDRAW));
        let weak = cx.weak_entity();
        // The list calls this while it holds its own state: only schedule work here.
        state.set_scroll_handler(move |_, _, cx| {
            let _ = weak.update(cx, |view, cx| view.settle_stream_focus(cx));
        });
        let focus = cx.focus_handle();
        let mut stream = StreamScreen {
            focus: focus.clone(),
            model,
            rows,
            list: state,
            layout: Layout::SideBySide,
            look: Look::rich(),
            menu: None,
            details: false,
            sides: super::sides_label(self),
            outline: Outline::new(cx),
            search: None,
            control,
            requests: requests.into_iter().map(|r| (r.id, r)).collect(),
            stamp: self.browser.compare_mode().map(|mode| mode.rows_stamp()),
            resynced: None,
            resync_wait: None,
            rerun: Rerun::default(),
            inbox,
            sent: None,
            settle: None,
        };
        stream.send_focus();
        let screen = self
            .compare
            .screen
            .get_or_insert_with(CompareScreen::default);
        screen.stream = Some(stream);
        cx.notify();
        Some(focus)
    }

    /// Opens the Stream over `items` (the list and each item's pair) and focuses it; false when
    /// the app has no diff hooks.
    pub(in crate::window_view) fn open_stream(
        &mut self,
        items: (StreamList, Vec<PairRequest>),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(focus) = self.start_stream(items, cx) else {
            return false;
        };
        window.defer(cx, move |window, cx| window.focus(&focus, cx));
        true
    }

    /// Drains the Stream's inbox into its model, drops arrived cards far from the view and
    /// splices the rows that changed. Returns whether anything arrived.
    pub(super) fn poll_stream(&mut self, cx: &mut Context<Self>) -> bool {
        let resynced = self.resync_stream(cx);
        let Some(stream) = self.stream_mut() else {
            return false;
        };
        let updates =
            std::mem::take(&mut *stream.inbox.lock().unwrap_or_else(PoisonError::into_inner));
        for update in &updates {
            stream.model.apply(update);
        }
        if updates.is_empty() {
            return resynced;
        }
        if updates
            .iter()
            .any(|u| matches!(u, StreamUpdate::Card { .. }))
        {
            stream.drop_far_cards();
        }
        stream.sync_rows();
        true
    }
}

#[cfg(test)]
impl NavoWindow {
    /// The Stream's model, rows and list, for tests.
    pub(in crate::window_view) fn stream_probe(
        &self,
    ) -> Option<(&StreamModel, &[StreamRow], &ListState)> {
        let stream = self.compare.screen.as_ref()?.stream.as_ref()?;
        Some((&stream.model, &stream.rows, &stream.list))
    }

    /// The Stream's look and, while the details drawer is open, its lines.
    pub(in crate::window_view) fn stream_look(
        &self,
    ) -> Option<(super::look::Look, Option<Vec<String>>)> {
        let stream = self.compare.screen.as_ref()?.stream.as_ref()?;
        let details = stream.details.then(|| details::lines(stream));
        Some((stream.look, details))
    }

    /// Runs the settle step a scroll of the list starts.
    pub(in crate::window_view) fn stream_scrolled(&mut self, cx: &mut Context<Self>) {
        self.settle_stream_focus(cx);
    }
}
