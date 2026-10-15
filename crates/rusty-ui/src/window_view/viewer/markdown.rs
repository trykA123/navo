use std::any::Any;
use std::sync::{Arc, Mutex};

use gpui_kit::component::text::{TextView, TextViewState};
use gpui_kit::{AnyView, ClickEvent, FocusHandle, KeyBinding, ListOffset, Stateful};

use super::body::{BodyKeys, Host, ViewerBody};
use super::rich_header::{HeaderParts, header};
use super::{GoEnd, GoStart, PageDown, PageUp, RowDown, RowUp};
use crate::prelude::*;
use crate::tokens;

const CONTEXT: &str = "Markdown";
const LINE_STEP: f32 = 3.;

gpui_kit::actions!(viewer_markdown, [OpenLink]);

pub(super) fn bind_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("enter", OpenLink, Some(CONTEXT))]);
}

pub(super) struct MarkdownFile {
    pub text: String,
    pub len: u64,
}

type Picked = Arc<Mutex<Option<SharedString>>>;

pub(super) struct MarkdownView {
    title: SharedString,
    len: u64,
    focus: FocusHandle,
    state: Entity<TextViewState>,
    picked: Picked,
    note: Option<SharedString>,
}

fn picked_link(picked: &Picked) -> Option<SharedString> {
    picked.lock().ok().and_then(|link| link.clone())
}

fn on_link(picked: Picked) -> impl Fn(&SharedString, &ClickEvent, &mut Window, &mut App) {
    move |url, event, window, cx| {
        if event.is_keyboard() || event.modifiers().secondary() {
            cx.open_url(url);
            return;
        }
        if let Ok(mut link) = picked.lock() {
            *link = Some(url.clone());
        }
        window.refresh();
    }
}

impl MarkdownView {
    fn new(file: MarkdownFile, host: &Host, cx: &mut Context<Self>) -> Self {
        Self {
            title: host.title.clone(),
            len: file.len,
            focus: cx.focus_handle(),
            state: cx.new(|cx| TextViewState::markdown(&file.text, cx)),
            picked: Picked::default(),
            note: None,
        }
    }

    fn line(cx: &App) -> Pixels {
        tokens::current(cx).typography.line_height
    }

    fn scroll_by(&mut self, distance: Pixels, cx: &mut Context<Self>) {
        self.state.read(cx).list_state().scroll_by(distance);
        cx.notify();
    }

    fn page(window: &Window) -> Pixels {
        window.viewport_size().height * 0.8
    }

    fn scroll_to(&mut self, end: bool, cx: &mut Context<Self>) {
        let list = self.state.read(cx).list_state().clone();
        if end {
            list.scroll_to_end();
        } else {
            list.scroll_to(ListOffset::default());
        }
        cx.notify();
    }

    fn open_link(&mut self, cx: &mut Context<Self>) {
        match picked_link(&self.picked) {
            Some(url) => {
                cx.open_url(&url);
                self.note = Some(format!("Opened {url}").into());
                cx.notify();
            }
            None => cx.propagate(),
        }
    }

    fn keys(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &RowUp, _, cx| {
            view.scroll_by(-Self::line(cx) * LINE_STEP, cx);
        }))
        .on_action(cx.listener(|view, _: &RowDown, _, cx| {
            view.scroll_by(Self::line(cx) * LINE_STEP, cx);
        }))
        .on_action(cx.listener(|view, _: &PageUp, window, cx| {
            view.scroll_by(-Self::page(window), cx);
        }))
        .on_action(cx.listener(|view, _: &PageDown, window, cx| {
            view.scroll_by(Self::page(window), cx);
        }))
        .on_action(cx.listener(|view, _: &GoStart, _, cx| view.scroll_to(false, cx)))
        .on_action(cx.listener(|view, _: &GoEnd, _, cx| view.scroll_to(true, cx)))
        .on_action(cx.listener(|view, _: &OpenLink, _, cx| view.open_link(cx)))
    }
}

impl Render for MarkdownView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let picked = picked_link(&self.picked);
        let parts = HeaderParts {
            title: self.title.clone(),
            mode: "Markdown".into(),
            len: self.len,
            detail: picked.map(|url| ("markdown-link", format!("Enter opens {url}").into())),
            status: self.note.clone().map(|note| ("markdown-status", note)),
        };
        let text = TextView::new(&self.state)
            .scrollable(true)
            .on_link_click(on_link(self.picked.clone()));
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
            .child(header(&t, parts))
            .child(
                div()
                    .id("markdown-body")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .h_full()
                            .w_full()
                            .max_w(t.spacing.rail_width * 3.5)
                            .px(t.spacing.xxl)
                            .pt(t.spacing.xl)
                            .child(text),
                    ),
            )
    }
}

pub(super) struct MarkdownBody(pub Entity<MarkdownView>);

impl MarkdownBody {
    pub(super) fn new(file: MarkdownFile, host: Host, cx: &mut App) -> Self {
        Self(cx.new(|cx| MarkdownView::new(file, &host, cx)))
    }

    #[cfg(test)]
    pub(super) fn rendered(&self, cx: &App) -> String {
        let state = self.0.read(cx).state.read(cx);
        state.rendered_text().as_str().to_owned()
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
            ("Ctrl+Click", "Open link"),
            ("Enter", "Open picked link"),
        ])
    }

    fn find_again(&self, _: bool, _: &mut Window, _: &mut App) {}

    fn set_note(&self, note: SharedString, cx: &mut App) {
        self.0.update(cx, |view, cx| {
            view.note = Some(note);
            cx.notify();
        });
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
