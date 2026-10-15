use std::ops::Range;

use gpui_kit::component::input::InputEvent;
use gpui_kit::{Focusable, Subscription};
use rusty_core::paging::{Found, Needle, Search, hex_row_start, parse_hex};

use super::PagedView;
use super::jobs::{Job, SearchMessage, start_search};
use super::lines::{PagedKind, row_start};
use crate::prelude::*;
use crate::primitives::{TextFieldState, text_field_state};

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
    last: Option<String>,
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

fn needle_width(needle: &Needle) -> u64 {
    match needle {
        Needle::Text { text, .. } => text.len() as u64,
        Needle::Bytes(bytes) => bytes.len() as u64,
    }
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

    fn needle(&self, text: &str) -> Result<Needle, String> {
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

    pub(super) fn search_message(&mut self, message: SearchMessage, cx: &mut Context<Self>) {
        let Some(progress) = &mut self.find.progress else {
            return;
        };
        match message {
            SearchMessage::Progress(done) => progress.done = done,
            SearchMessage::Done(result) => {
                let (backward, width) = (progress.backward, progress.width);
                self.find.progress = None;
                self.found(result, backward, width);
            }
        }
        cx.notify();
    }

    fn found(&mut self, result: Result<Found, String>, backward: bool, width: u64) {
        match result {
            Ok(Found::At(at)) => match row_start(&*self.source, self.kind, at) {
                Ok(top) => {
                    self.top = top;
                    self.find.hit = Some(at..at + width);
                    self.find.anchor = Some(at);
                }
                Err(error) => self.find.note = Some(error.into()),
            },
            Ok(Found::NotFound) if backward => self.find.note = Some("Not found above.".into()),
            Ok(Found::NotFound) => self.find.note = Some("Not found below.".into()),
            Ok(Found::Cancelled) => {}
            Err(error) => self.find.note = Some(error.into()),
        }
    }

    fn go_to(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        match self.go_to_target(text) {
            Ok(top) => {
                self.top = top;
                self.find.anchor = None;
                self.close_bar(window, cx);
            }
            Err(error) => self.set_bar_error(Some(error), cx),
        }
    }

    fn go_to_target(&self, text: &str) -> Result<u64, String> {
        let source = &*self.source;
        if self.kind == PagedKind::Hex {
            let offset = parse_offset(text)?;
            return Ok(hex_row_start(offset.min(source.len().saturating_sub(1))));
        }
        let line = parse_line(text)?;
        let found = match &self.index {
            Some(index) => index.line_start(source, line)?,
            None => None,
        };
        match found {
            Some(start) => Ok(start),
            None if self.index_done => Err(format!("There is no line {line}.")),
            None => Err("Not indexed yet.".to_owned()),
        }
    }
}
