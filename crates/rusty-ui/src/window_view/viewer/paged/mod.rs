mod body;
mod draw;
mod find;
mod follow;
mod found;
mod jobs;
mod lines;
mod marking;
mod moves;
mod row_view;
mod rows;
mod scroll;
mod spans;
mod view;

use std::collections::HashMap;
use std::sync::Arc;

use gpui_kit::{FocusHandle, Focusable, ScrollDelta, ScrollWheelEvent, Task};
use rusty_core::paging::{LineIndex, MAX_ROW};

pub(super) use body::PagedBody;
pub use lines::PagedKind;

use super::files::{IndexRun, Paged, Reopen, SharedSource};
use super::search::find_all::FoundRow;
use super::search::jobs::Job;
use super::search::memory::{MemoryLink, attach_memory};
use super::search::overview::Overview;
use super::search::{Marking, source_changed};
use crate::prelude::*;
use find::{BarKind, FindState};
use follow::Follower;
use jobs::{IndexDone, IndexMessage};
use moves::{Move, MoveQueue};
use rows::RowCache;
use scroll::ByteScroll;

const WHEEL_ROWS: f32 = 3.;
const SHIFT_COLUMNS: usize = 8;

pub struct PagedView {
    title: SharedString,
    summary: Option<SharedString>,
    source: SharedSource,
    reopen: Reopen,
    index_run: IndexRun,
    pub(super) kind: PagedKind,
    pub(super) top: u64,
    moves: MoveQueue,
    shift: usize,
    focus: FocusHandle,
    scroll: ByteScroll,
    pub(super) rows: u64,
    row_height: Pixels,
    wheel: f32,
    changed: bool,
    note: Option<SharedString>,
    index: Option<Arc<LineIndex>>,
    index_rev: u64,
    source_rev: u64,
    cache: RowCache,
    index_done: bool,
    index_failed: bool,
    line_count: Option<u64>,
    index_job: Option<Job>,
    find: FindState,
    marking: Marking,
    follower: Follower,
    overview: Overview,
    previews: HashMap<u64, FoundRow>,
    preview_job: Option<Task<()>>,
}

impl PagedView {
    pub fn new(
        title: impl Into<SharedString>,
        paged: Paged,
        kind: PagedKind,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut view = Self {
            title: title.into(),
            summary: paged.summary.map(SharedString::from),
            source: paged.source,
            marking: Marking::new(paged.reopen.clone()),
            reopen: paged.reopen,
            index_run: paged.index,
            kind,
            top: 0,
            moves: MoveQueue::default(),
            shift: 0,
            focus: cx.focus_handle(),
            scroll: ByteScroll::default(),
            rows: 1,
            row_height: px(0.),
            wheel: 0.,
            changed: false,
            note: None,
            index: None,
            index_rev: 0,
            source_rev: 0,
            cache: RowCache::default(),
            index_done: false,
            index_failed: false,
            line_count: None,
            index_job: None,
            find: FindState::default(),
            follower: Follower::default(),
            overview: Overview::default(),
            previews: HashMap::new(),
            preview_job: None,
        };
        view.start_index(cx);
        view
    }

    pub fn attach_memory(&mut self, link: MemoryLink, cx: &mut Context<Self>) {
        attach_memory(self, link, cx);
    }

    pub fn find_all(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.open_bar(BarKind::Find, window, cx);
        if let Some(bar) = &self.find.bar {
            bar.input
                .update(cx, |input, cx| input.set_value(text.to_owned(), window, cx));
        }
        self.find_all_in_bar(window, cx);
    }

    fn start_index(&mut self, cx: &mut Context<Self>) {
        if self.kind == PagedKind::Text && self.index_job.is_none() {
            let source = self.source.clone();
            self.index_job = Some(jobs::start_index(self.index_run.clone(), source, cx));
        }
    }

    fn index_message(&mut self, message: IndexMessage, cx: &mut Context<Self>) {
        match message {
            IndexMessage::Snapshot(snapshot) => self.index = Some(Arc::new(snapshot)),
            IndexMessage::Done(IndexDone {
                result: Ok(Some(index)),
                line_count,
            }) => {
                self.line_count = line_count;
                self.index = Some(Arc::new(index));
                self.index_done = true;
            }
            IndexMessage::Done(IndexDone {
                result: Ok(None), ..
            }) => {}
            IndexMessage::Done(IndexDone {
                result: Err(error), ..
            }) => {
                self.index_failed = true;
                self.note = Some(error.into());
            }
        }
        self.index_rev += 1;
        cx.notify();
    }

    pub(super) fn set_kind(&mut self, kind: PagedKind, cx: &mut Context<Self>) {
        if self.kind == kind {
            return;
        }
        self.kind = kind;
        self.shift = 0;
        self.find.leave_kind();
        self.marking.count = None;
        self.marking.find_all = None;
        self.previews.clear();
        if kind == PagedKind::Hex {
            self.stop_following();
        }
        self.queue_move(Move::Realign, cx);
        self.start_index(cx);
    }

    pub(super) fn set_note(&mut self, note: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.note = Some(note.into());
        cx.notify();
    }

    fn queue_move(&mut self, to: Move, cx: &mut Context<Self>) {
        self.moves.push(to);
        self.request_rows(cx);
        cx.notify();
    }

    fn user_move(&mut self, to: Move, cx: &mut Context<Self>) {
        self.follow_moved(to);
        self.find.anchor = None;
        self.queue_move(to, cx);
    }

    fn scroll_rows(&mut self, rows: i64, cx: &mut Context<Self>) {
        self.user_move(Move::Rows(rows), cx);
    }

    fn page(&self) -> i64 {
        i64::try_from(self.rows).unwrap_or(i64::MAX)
    }

    fn shift_by(&mut self, right: bool, cx: &mut Context<Self>) {
        if self.kind != PagedKind::Text {
            return;
        }
        let limit = usize::try_from(MAX_ROW).unwrap_or(usize::MAX);
        self.shift = if right {
            (self.shift + SHIFT_COLUMNS).min(limit)
        } else {
            self.shift.saturating_sub(SHIFT_COLUMNS)
        };
        cx.notify();
    }

    fn scroll_wheel(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        if self.row_height <= px(0.) {
            return;
        }
        let rows = match event.delta {
            ScrollDelta::Lines(delta) => -delta.y * WHEEL_ROWS,
            ScrollDelta::Pixels(delta) => -(delta.y / self.row_height),
        };
        self.wheel += rows;
        let whole = self.wheel.trunc();
        if whole != 0. {
            self.wheel -= whole;
            self.scroll_rows(whole as i64, cx);
        }
    }

    fn follow_drag(&mut self, cx: &mut Context<Self>) {
        if let Some(fraction) = self.scroll.take_drag() {
            self.user_move(Move::Fraction(fraction), cx);
        }
    }

    fn follow_changes(&mut self, cx: &mut Context<Self>) {
        if !self.source.take_changed() {
            return;
        }
        self.changed = true;
        self.source_rev += 1;
        self.previews.clear();
        source_changed(self, cx);
        let len = self.source.len();
        if self.top >= len {
            self.queue_move(Move::Offset(len), cx);
        } else {
            cx.notify();
        }
        self.follow_grew(cx);
    }
}

#[cfg(test)]
impl PagedView {
    pub(super) fn fills(&self) -> u64 {
        self.cache.fills
    }

    pub(super) fn marking_state(&self) -> &Marking {
        &self.marking
    }

    pub(super) fn bar_text(&self, cx: &App) -> Option<String> {
        let bar = self.find.bar.as_ref()?;
        Some(bar.input.read(cx).value().to_string())
    }
}

impl Focusable for PagedView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
