//! The full-window compare screen (D23): the file diff stacked over the Stream, drawn where the
//! viewer draws. This file holds its state: how a file diff opens, takes updates and pops.

mod context;
mod copy;
mod details;
mod drawers;
mod entity;
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
mod open;
mod outline;
mod pick;
mod resync;
mod search;
mod sheet;
pub(in crate::window_view::compare_screen) mod sideways;
mod state;
mod stream;
pub(in crate::window_view::compare_screen) mod strip;
mod switch;

use std::sync::Arc;

use gpui_kit::{FocusHandle, Subscription};

use super::ScreenOwner;
use crate::compare_diff::{DiffUpdate, Layout, PairControl, PairKind};
use crate::compare_diff::{PairRequest, PairText};
use crate::context_menu::ContextMenu;
use crate::prelude::*;
pub(in crate::window_view) use entity::ScreenEvent;
pub(in crate::window_view) use entry::items_of;
pub(in crate::window_view) use entry::neighbour;
pub(in crate::window_view) use inbox::DiffSlot;
#[cfg(test)]
pub(in crate::window_view) use inbox::coalesce;
use large::LargeShown;
use sideways::SidewaysHandle;
pub(super) use state::FileState;
use state::request_of;
use stream::StreamScreen;

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
    /// The Alt+V switch in flight and the view the pair is in (see `switch`).
    switch: switch::Switch,
    /// The large text snapshot applied last; its number was acknowledged after applying.
    large: Option<LargeShown>,
    /// The sideways state of the large body a `refresh` replaced; the body built for the same
    /// pair carries it, and it is dropped on any other `Opened` (a new pair starts at 0).
    carried_sideways: Option<SidewaysHandle>,
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
                self.switch.landed();
                self.switch.release();
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
        let landing = self.switch.landed();
        let old_view = self.view.take();
        let (at, words) = (self.at.filter(|_| landing.is_none()), self.look.words);
        self.view = text
            .clone()
            .map(|text| file::FileView::new(text, at, words));
        if let (Some(view), Some(old)) = (self.view.as_mut(), old_view.as_ref()) {
            view.keep_sideways(old);
            view.keep_syntax(old);
        }
        let layout = self.look.layout.unwrap_or(self.auto);
        let sideways = self.take_sideways();
        self.large = matches!(kind, PairKind::Large { .. })
            .then(|| LargeShown::new(row_height, layout, sideways));
        let old = self.hex.take();
        self.hex = matches!(kind, PairKind::Binary).then(|| hex::HexView::reopened(old));
        landing.inspect(|&at| self.land(at));
        self.open_pictures(kind);
        self.state = FileState::Opened { kind, text };
        self.follow_layout();
        match (landing, self.hex.is_some()) {
            (Some(_), shown) => self.switch.hold(old_view, shown),
            (None, false) => self.switch.release(),
            (None, true) => {}
        }
        self.drop_unfit_bytes(kind);
    }

    /// Asks the worker to reload both sides under the next request id; updates for every earlier
    /// id become stale. Fails the pair, without reusing an id, once the ids run out. Keeps the
    /// large body's sideways state for the body the reload builds (W8j2).
    fn refresh(&mut self) {
        if let Some(large) = self.large.as_ref() {
            self.carried_sideways = Some(large.sideways());
        }
        if let Some(id) = self.next_id() {
            self.control.refresh(id);
        }
    }

    /// The sideways state the next large body starts from: the one `refresh` kept, `carried`
    /// into it, or a fresh handle — offset 0 — when nothing was kept.
    pub(in crate::window_view::compare_screen) fn take_sideways(&mut self) -> SidewaysHandle {
        self.carried_sideways
            .take()
            .map_or_else(SidewaysHandle::default, |old| SidewaysHandle::carried(&old))
    }

    /// Takes the next request id and makes every earlier one stale; the pair fails, without
    /// reusing an id, once the ids run out (`None`).
    fn next_id(&mut self) -> Option<u64> {
        let next = self.expected.and_then(|id| id.checked_add(1));
        (self.expected, self.large) = (next, None);
        match next {
            Some(id) => self.active = id,
            None => {
                self.view = None;
                self.state = FileState::Failed(navo_l10n::tr("compare-could-not-start").into());
            }
        }
        next
    }
}

/// The screen in place of the panes: a Stream, a file diff on top of it, or both.
#[derive(Default)]
pub(super) struct ScreenStack {
    stream: Option<StreamScreen>,
    file: Option<FileScreen>,
}

#[cfg(test)]
mod probes;
#[cfg(test)]
mod resync_tests;
#[cfg(test)]
pub(in crate::window_view) use file::SidewaysSeen;
#[cfg(test)]
pub(in crate::window_view) use hex::HexProbe;
