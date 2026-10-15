use std::ops::Range;

use gpui_kit::component::input::InputEvent;
use gpui_kit::{Focusable, Subscription};
use rusty_core::paging::{Found, Needle, Search, parse_hex};

use super::PagedView;
use super::jobs::{SearchMessage, start_search};
use super::lines::PagedKind;
use super::moves::Move;
use crate::prelude::*;
use crate::primitives::{TextFieldState, text_field_state};
use crate::window_view::viewer::search::count::{count_search, needle_width};
use crate::window_view::viewer::search::find_all::open_find_all;
use crate::window_view::viewer::search::history::Recall;
use crate::window_view::viewer::search::jobs::Job;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BarKind {
    Find,
    GoTo,
}

pub(super) struct Bar {
    pub kind: BarKind,
    pub input: Entity<TextFieldState>,
    pub error: Option<SharedString>,
    _enter: Subscription,
}

pub(super) struct Progress {
    pub done: u64,
    pub total: u64,
    backward: bool,
    width: u64,
}

#[derive(Default)]
pub(super) struct FindState {
    pub bar: Option<Bar>,
    pub match_case: bool,
    pub progress: Option<Progress>,
    pub hit: Option<Range<u64>>,
    pub anchor: Option<u64>,
    pub note: Option<SharedString>,
    pub last: Option<String>,
    parked: Option<(String, bool)>,
    job: Option<Job>,
}

impl FindState {
    pub(super) fn leave_kind(&mut self) {
        self.cancel();
        self.bar = None;
        self.hit = None;
        self.anchor = None;
        self.note = None;
        self.last = None;
        self.parked = None;
    }

    fn cancel(&mut self) {
        self.job = None;
        self.progress = None;
    }
}

fn placeholder(bar: BarKind, kind: PagedKind) -> &'static str {
    match (bar, kind) {
        (BarKind::Find, PagedKind::Hex) => "Hex bytes, like 4D 5A",
        (BarKind::Find, PagedKind::Text) => "Text to find",
        (BarKind::GoTo, PagedKind::Hex) => "Hex offset, like 1F00",
        (BarKind::GoTo, PagedKind::Text) => "Line number",
    }
}

fn parse_offset(text: &str) -> Result<u64, String> {
    let trimmed = text.trim();
    let digits = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    u64::from_str_radix(digits, 16).map_err(|_| format!("Not a hex offset: {trimmed}"))
}

fn parse_line(text: &str) -> Result<u64, String> {
    let trimmed = text.trim();
    trimmed
        .parse::<u64>()
        .ok()
        .filter(|line| *line > 0)
        .ok_or_else(|| format!("Not a line number: {trimmed}"))
}

impl PagedView {
    pub(super) fn open_bar(&mut self, kind: BarKind, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(bar) = self.find.bar.as_ref().filter(|bar| bar.kind == kind) {
            let focus = bar.input.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
            return;
        }
        let input = text_field_state(placeholder(kind, self.kind), window, cx);
        let enter = cx.subscribe_in(&input, window, |view, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { shift, .. } = event {
                view.submit_bar(*shift, window, cx);
            }
        });
        let focus = input.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        self.find.bar = Some(Bar {
            kind,
            input,
            error: None,
            _enter: enter,
        });
        cx.notify();
    }

    fn close_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.find.bar = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(super) fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.find.progress.is_some() {
            self.find.cancel();
            cx.notify();
        } else if self.find.bar.is_some() {
            self.close_bar(window, cx);
        } else {
            cx.propagate();
        }
    }

    pub(super) fn toggle_case(&mut self, cx: &mut Context<Self>) {
        self.find.match_case = !self.find.match_case;
        cx.notify();
    }

    fn set_bar_error(&mut self, error: Option<String>, cx: &mut Context<Self>) {
        if let Some(bar) = &mut self.find.bar {
            bar.error = error.map(SharedString::from);
        }
        cx.notify();
    }

    fn submit_bar(&mut self, backward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(bar) = &self.find.bar else {
            return;
        };
        let text = bar.input.read(cx).value().to_string();
        match bar.kind {
            BarKind::Find => self.start_find(&text, backward, cx),
            BarKind::GoTo => self.go_to(&text, window, cx),
        }
    }

    pub(super) fn needle(&self, text: &str) -> Result<Needle, String> {
        match self.kind {
            PagedKind::Hex => parse_hex(text).map(Needle::Bytes),
            PagedKind::Text if text.is_empty() => Err("Type the text to find.".to_owned()),
            PagedKind::Text => Ok(Needle::Text {
                text: text.to_owned(),
                match_case: self.find.match_case,
            }),
        }
    }

    pub(in crate::window_view::viewer) fn find_again(
        &mut self,
        backward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let typed = self
            .find
            .bar
            .as_ref()
            .filter(|bar| bar.kind == BarKind::Find)
            .map(|bar| bar.input.read(cx).value().to_string())
            .filter(|text| !text.is_empty());
        match typed.or_else(|| self.find.last.clone()) {
            Some(text) => self.start_find(&text, backward, cx),
            None => self.open_bar(BarKind::Find, window, cx),
        }
    }

    fn start_find(&mut self, text: &str, backward: bool, cx: &mut Context<Self>) {
        let needle = match self.needle(text) {
            Ok(needle) => needle,
            Err(error) => return self.set_bar_error(Some(error), cx),
        };
        self.find.last = Some(text.to_owned());
        count_search(self, needle.clone(), cx);
        self.marking.remember_search(&needle, cx);
        if !self.moves.is_empty() {
            self.find.parked = Some((text.to_owned(), backward));
            return self.set_bar_error(None, cx);
        }
        let from = match (self.find.anchor, backward) {
            (Some(anchor), false) => anchor + 1,
            (Some(anchor), true) => anchor,
            (None, _) => self.top,
        };
        let len = self.source.len();
        self.find.progress = Some(Progress {
            done: 0,
            total: if backward {
                from
            } else {
                len.saturating_sub(from)
            },
            backward,
            width: needle_width(&needle),
        });
        self.find.note = None;
        let search = Search {
            needle,
            from,
            backward,
        };
        self.find.job = Some(start_search(self.reopen.clone(), search, cx));
        self.set_bar_error(None, cx);
    }

    pub(super) fn resume_find(&mut self, cx: &mut Context<Self>) {
        if let Some((text, backward)) = self.find.parked.take() {
            self.start_find(&text, backward, cx);
        }
    }

    pub(super) fn search_message(&mut self, message: SearchMessage, cx: &mut Context<Self>) {
        let Some(progress) = &mut self.find.progress else {
            return;
        };
        match message {
            SearchMessage::Progress(done) => progress.done = done,
            SearchMessage::Done(result) => {
                if let Some(search) = self.find.progress.take() {
                    self.found(result, search, cx);
                }
            }
        }
        cx.notify();
    }

    fn found(&mut self, result: Result<Found, String>, search: Progress, cx: &mut Context<Self>) {
        match result {
            Ok(Found::At(at)) => {
                self.queue_move(Move::Offset(at), cx);
                self.find.hit = Some(at..at + search.width);
                self.find.anchor = Some(at);
            }
            Ok(Found::NotFound) if search.backward => {
                self.find.note = Some("Not found above.".into());
            }
            Ok(Found::NotFound) => self.find.note = Some("Not found below.".into()),
            Ok(Found::Cancelled) => {}
            Err(error) => self.find.note = Some(error.into()),
        }
    }

    pub(super) fn recall(&mut self, older: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(bar) = self
            .find
            .bar
            .as_ref()
            .filter(|bar| bar.kind == BarKind::Find)
        else {
            return;
        };
        let input = bar.input.clone();
        let recall = Recall {
            older,
            kind: self.needle_kind(),
            input: &input,
        };
        if let Some(Some(match_case)) = self.marking.history.recall_into(recall, window, cx) {
            self.find.match_case = match_case;
        }
        cx.notify();
    }

    pub(super) fn find_all_in_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(bar) = self
            .find
            .bar
            .as_ref()
            .filter(|bar| bar.kind == BarKind::Find)
        else {
            return;
        };
        let text = bar.input.read(cx).value().to_string();
        match self.needle(&text) {
            Ok(needle) => {
                self.set_bar_error(None, cx);
                self.marking.remember_search(&needle, cx);
                self.find.last = Some(text);
                open_find_all(self, needle, window, cx);
            }
            Err(error) => self.set_bar_error(Some(error), cx),
        }
    }

    fn go_to(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let target = match self.kind {
            PagedKind::Hex => parse_offset(text).map(Move::Offset),
            PagedKind::Text => parse_line(text).map(Move::Line),
        };
        match target {
            Ok(target) => {
                self.note = None;
                self.close_bar(window, cx);
                self.user_move(target, cx);
            }
            Err(error) => self.set_bar_error(Some(error), cx),
        }
    }
}
