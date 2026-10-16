mod block;
mod links;
mod runs;
mod table;

use std::any::Any;
use std::rc::Rc;

use gpui_kit::base::{Scrollbar, ScrollbarMode};
use gpui_kit::{
    AnyView, FocusHandle, KeyBinding, ListAlignment, ListOffset, ListState, Stateful, list,
};
use rusty_core::Location;
use rusty_structured::markdown::Document;

use super::background::{Job, spawn_job};
use super::body::{BodyKeys, Host, ViewerBody};
use super::rich_header::{HeaderParts, header};
use super::{GoEnd, GoStart, PageDown, PageUp, RowDown, RowUp};
use crate::prelude::*;
use crate::tokens;
use block::{Page, draw_block};

const CONTEXT: &str = "Markdown";
const LINE_STEP: f32 = 3.;
const OVERDRAW: f32 = 200.;

gpui_kit::actions!(viewer_markdown, [OpenLink, NextLink, PreviousLink]);

pub(super) fn bind_keys(cx: &mut App) {
    let context = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("enter", OpenLink, context),
        KeyBinding::new("tab", NextLink, context),
        KeyBinding::new("shift-tab", PreviousLink, context),
    ]);
}

pub(super) struct MarkdownFile {
    pub doc: Document,
    pub len: u64,
}

pub(super) struct MarkdownView {
    title: SharedString,
    len: u64,
    host: Host,
    focus: FocusHandle,
    doc: Rc<Document>,
    list: ListState,
    picked: Option<u32>,
    note: Option<SharedString>,
    checking: Option<Job>,
    #[cfg(test)]
    drawn: Rc<std::cell::Cell<usize>>,
}

impl MarkdownView {
    fn new(file: MarkdownFile, host: Host, cx: &mut Context<Self>) -> Self {
        let count = file.doc.blocks().len();
        Self {
            title: host.title.clone(),
            len: file.len,
            host,
            focus: cx.focus_handle(),
            doc: Rc::new(file.doc),
            list: ListState::new(count, ListAlignment::Top, px(OVERDRAW)),
            picked: None,
            note: None,
            checking: None,
            #[cfg(test)]
            drawn: Rc::default(),
        }
    }

    fn say(&mut self, note: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.note = Some(note.into());
        cx.notify();
    }

    fn target(&self, link: u32) -> Option<SharedString> {
        let target = self.doc.link(link)?.target();
        Some(SharedString::from(target.to_owned()))
    }

    pub(super) fn click_link(&mut self, link: u32, opens: bool, cx: &mut Context<Self>) {
        self.picked = Some(link);
        if opens {
            self.follow_picked(cx);
        } else {
            cx.notify();
        }
    }

    fn follow_picked(&mut self, cx: &mut Context<Self>) {
        let Some(url) = self.picked.and_then(|link| self.target(link)) else {
            cx.propagate();
            return;
        };
        match links::classify(&url) {
            links::Link::Web => {
                cx.open_url(&url);
                self.say(format!("Opened {url}"), cx);
            }
            links::Link::Local(segments) => match links::resolve(&self.host.location, &segments) {
                Some(target) => self.open_if_found(target, cx),
                None => self.say(format!("Cannot open {url}: not a file name."), cx),
            },
            links::Link::Refused => {
                self.say(format!("Not opened: {url} is not a web or file link."), cx);
            }
        }
    }

    fn open_if_found(&mut self, target: Location, cx: &mut Context<Self>) {
        let head = self.host.hooks.head.clone();
        self.checking = Some(spawn_job(
            cx,
            move |_, send| {
                let found = head(&target).is_ok();
                send((target, found));
            },
            |view: &mut Self, (target, found): (Location, bool), cx| {
                view.checking = None;
                if found {
                    (view.host.open)(target, cx);
                } else {
                    view.say(format!("Not found: {}", target.as_str()), cx);
                }
            },
        ));
    }

    fn step_link(&mut self, backward: bool, cx: &mut Context<Self>) {
        let count = u32::try_from(self.doc.links().len()).unwrap_or(u32::MAX);
        if count == 0 {
            self.say("No links in this file.", cx);
            return;
        }
        let next = match (self.picked, backward) {
            (None, false) => 0,
            (None, true) => count - 1,
            (Some(at), false) => (at + 1) % count,
            (Some(at), true) => at.checked_sub(1).unwrap_or(count - 1),
        };
        self.picked = Some(next);
        if let Some(link) = self.doc.link(next) {
            self.list.scroll_to_reveal_item(link.block());
        }
        cx.notify();
    }

    fn scroll_by(&mut self, distance: Pixels, cx: &mut Context<Self>) {
        self.list.scroll_by(distance);
        cx.notify();
    }

    fn scroll_to(&mut self, end: bool, cx: &mut Context<Self>) {
        if end {
            self.list.scroll_to_end();
        } else {
            self.list.scroll_to(ListOffset::default());
        }
        cx.notify();
    }

    fn keys(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        let line = tokens::current(cx).typography.line_height * LINE_STEP;
        root.on_action(cx.listener(move |view, _: &RowUp, _, cx| view.scroll_by(-line, cx)))
            .on_action(cx.listener(move |view, _: &RowDown, _, cx| view.scroll_by(line, cx)))
            .on_action(cx.listener(|view, _: &PageUp, window, cx| {
                view.scroll_by(-window.viewport_size().height * 0.8, cx);
            }))
            .on_action(cx.listener(|view, _: &PageDown, window, cx| {
                view.scroll_by(window.viewport_size().height * 0.8, cx);
            }))
            .on_action(cx.listener(|view, _: &GoStart, _, cx| view.scroll_to(false, cx)))
            .on_action(cx.listener(|view, _: &GoEnd, _, cx| view.scroll_to(true, cx)))
            .on_action(cx.listener(|view, _: &OpenLink, _, cx| view.follow_picked(cx)))
            .on_action(cx.listener(|view, _: &NextLink, _, cx| view.step_link(false, cx)))
            .on_action(cx.listener(|view, _: &PreviousLink, _, cx| view.step_link(true, cx)))
    }

    fn blocks(&self, page: Page) -> impl IntoElement {
        #[cfg(test)]
        let drawn = self.drawn.clone();
        list(self.list.clone(), move |index, _, _| {
            #[cfg(test)]
            drawn.set(drawn.get() + 1);
            draw_block(&page, index)
        })
        .size_full()
    }
}

impl Render for MarkdownView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let picked = self.picked.and_then(|link| self.target(link));
        let parts = HeaderParts {
            title: self.title.clone(),
            mode: "Markdown".into(),
            len: self.len,
            detail: picked.map(|url| ("markdown-link", format!("Enter opens {url}").into())),
            status: self.note.clone().map(|note| ("markdown-status", note)),
        };
        let page = Page {
            t: t.clone(),
            doc: self.doc.clone(),
            picked: self.picked,
            view: cx.weak_entity(),
        };
        let root = div()
            .id("markdown")
            .key_context(CONTEXT)
            .track_focus(&self.focus);
        Self::keys(root, cx)
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .size_full()
            .bg(t.colors.surface)
            .text_color(t.colors.text)
            .text_size(t.typography.body)
            .line_height(t.typography.line_height)
            .child(header(&t, parts))
            .child(
                div()
                    .id("markdown-body")
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(self.blocks(page))
                    .child(
                        Scrollbar::vertical(&self.list)
                            .id("markdown-scrollbar")
                            .mode(ScrollbarMode::Always),
                    ),
            )
    }
}

pub(super) struct MarkdownBody(pub Entity<MarkdownView>);

impl MarkdownBody {
    pub(super) fn new(file: MarkdownFile, host: Host, cx: &mut App) -> Self {
        Self(cx.new(|cx| MarkdownView::new(file, host, cx)))
    }

    #[cfg(test)]
    pub(super) fn rendered(&self, cx: &App) -> String {
        let doc = &self.0.read(cx).doc;
        let texts: Vec<&str> = doc.blocks().iter().map(|block| doc.text(block)).collect();
        texts.join("\n")
    }

    #[cfg(test)]
    pub(super) fn drawn(&self, cx: &App) -> usize {
        self.0.read(cx).drawn.get()
    }
}

impl ViewerBody for MarkdownBody {
    fn view(&self) -> AnyView {
        self.0.clone().into()
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.0.read(cx).focus.clone()
    }

    fn keys(&self, _: &App) -> BodyKeys {
        BodyKeys::Normal(vec![
            ("Tab", "Next link"),
            ("Enter", "Open link"),
            ("Ctrl+Click", "Open link"),
        ])
    }

    fn find_again(&self, _: bool, _: &mut Window, _: &mut App) {}

    fn set_note(&self, note: SharedString, cx: &mut App) {
        self.0.update(cx, |view, cx| view.say(note, cx));
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
