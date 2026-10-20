use std::rc::Rc;
use std::time::Duration;

use gpui_kit::base::TestSupportExt;
use gpui_kit::component::input::EditorState;
use gpui_kit::{BackgroundExecutor, Focusable, Role, Stateful, Subscription, Task, WeakEntity};
use rusty_structured::markdown::{self, Document, MAX_SOURCE, Refusal};

use super::EditorView;
use crate::prelude::*;
use crate::tokens::Theme;
use crate::window_view::viewer::body::Host;
use crate::window_view::viewer::markdown::MarkdownView;
use crate::window_view::viewer::{FocusPreview, LeavePreview};

const PREVIEW_CONTEXT: &str = "MarkdownPreview";
const PREVIEW_AFTER: Duration = Duration::from_millis(150);
const INLINE_LIMIT: usize = 200 * 1024;
const SAVE_FIRST: &str = "Save or discard your edits before opening another file.";

pub(in crate::window_view::viewer) struct SplitChoice {
    pub is_shown: bool,
}

pub(super) struct Split {
    choice: SplitChoice,
    view: Entity<MarkdownView>,
    parsing: Option<Task<()>>,
    parsed: Option<u64>,
    synced: Option<u64>,
    refusal: Option<SharedString>,
    #[cfg(test)]
    pub renders: usize,
    _subscriptions: [Subscription; 2],
}

impl Split {
    pub(super) fn new(
        choice: SplitChoice,
        host: Host,
        editor: &Entity<EditorState>,
        cx: &mut Context<EditorView>,
    ) -> Self {
        let host = guarded(host, cx.weak_entity());
        let view = cx.new(|cx| MarkdownView::preview(host, cx));
        let cursor = cx.observe(editor, |view, _, cx| view.sync_preview(cx));
        let status = cx.observe(&view, |_, _, cx| cx.notify());
        Self {
            choice,
            view,
            parsing: None,
            parsed: None,
            synced: None,
            refusal: None,
            #[cfg(test)]
            renders: 0,
            _subscriptions: [cursor, status],
        }
    }

    #[cfg(test)]
    pub(super) fn view(&self) -> &Entity<MarkdownView> {
        &self.view
    }
}

fn guarded(host: Host, editor: WeakEntity<EditorView>) -> Host {
    let open = host.open.clone();
    Host {
        open: Rc::new(move |location, cx| {
            let is_dirty = editor.read_with(cx, |view, _| view.dirty).unwrap_or(true);
            if !is_dirty {
                return open(location, cx);
            }
            let preview = editor.read_with(cx, |view, _| {
                view.split.as_ref().map(|split| split.view.downgrade())
            });
            if let Ok(Some(preview)) = preview {
                cx.defer(move |cx| {
                    let _ = preview.update(cx, |view, cx| view.tell(SAVE_FIRST, cx));
                });
            }
        }),
        ..host
    }
}

fn parse(text: &str) -> Result<Document, Refusal> {
    markdown::parse(text)
}

async fn parse_off_thread(
    text: Option<String>,
    executor: BackgroundExecutor,
) -> Result<Document, Refusal> {
    match text {
        None => Err(Refusal::TooLarge),
        Some(text) if text.len() > INLINE_LIMIT => {
            executor.spawn(async move { parse(&text) }).await
        }
        Some(text) => parse(&text),
    }
}

impl EditorView {
    pub(super) fn has_split(&self) -> bool {
        self.split.is_some()
    }

    pub(super) fn is_split(&self) -> bool {
        self.editing
            && self
                .split
                .as_ref()
                .is_some_and(|split| split.choice.is_shown)
    }

    pub(super) fn is_preview_focused(&self, window: &Window, cx: &App) -> bool {
        self.split
            .as_ref()
            .is_some_and(|split| split.view.read(cx).focus().is_focused(window))
    }

    pub(super) fn preview_status(&self, cx: &App) -> Option<SharedString> {
        let split = self.split.as_ref().filter(|_| self.is_split())?;
        split
            .refusal
            .clone()
            .or_else(|| split.view.read(cx).status())
    }

    pub(super) fn show_split(&mut self, is_shown: bool, cx: &mut Context<Self>) {
        let Some(split) = self.split.as_mut() else {
            return;
        };
        if split.choice.is_shown == is_shown {
            return;
        }
        split.choice.is_shown = is_shown;
        if is_shown {
            self.refresh_preview(cx);
        }
        cx.notify();
    }

    fn focus_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(split) = self.split.as_ref().filter(|_| self.is_split()) else {
            return cx.propagate();
        };
        let focus = split.view.read(cx).focus().clone();
        window.focus(&focus, cx);
        cx.notify();
    }

    fn leave_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let focus = self.editor.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        cx.notify();
    }

    pub(super) fn preview_edited(&mut self, cx: &mut Context<Self>) {
        if self.is_split() {
            self.parse_later(PREVIEW_AFTER, cx);
        }
    }

    pub(super) fn refresh_preview(&mut self, cx: &mut Context<Self>) {
        let revision = self.revision;
        let Some(split) = self.split.as_ref().filter(|_| self.is_split()) else {
            return;
        };
        if split.parsed == Some(revision) {
            return self.sync_preview(cx);
        }
        let len = self.editor.read(cx).text().len();
        if len > MAX_SOURCE {
            return self.show_parsed(Err(Refusal::TooLarge), revision, cx);
        }
        if len > INLINE_LIMIT {
            return self.parse_later(Duration::ZERO, cx);
        }
        let parsed = parse(&self.editor.read(cx).text().to_string());
        self.show_parsed(parsed, revision, cx);
    }

    fn parse_later(&mut self, delay: Duration, cx: &mut Context<Self>) {
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let Ok((text, revision)) = this.update(cx, |view, cx| {
                let state = view.editor.read(cx);
                let text = (state.text().len() <= MAX_SOURCE).then(|| state.text().to_string());
                (text, view.revision)
            }) else {
                return;
            };
            let parsed = parse_off_thread(text, cx.background_executor().clone()).await;
            let _ = this.update(cx, |view, cx| view.show_parsed(parsed, revision, cx));
        });
        if let Some(split) = &mut self.split {
            split.parsing = Some(task);
        }
    }

    fn show_parsed(
        &mut self,
        parsed: Result<Document, Refusal>,
        revision: u64,
        cx: &mut Context<Self>,
    ) {
        let Some(split) = self.split.as_mut() else {
            return;
        };
        split.parsing = None;
        split.parsed = Some(revision);
        split.synced = None;
        #[cfg(test)]
        {
            split.renders += 1;
        }
        match parsed {
            Ok(doc) => {
                split.refusal = None;
                split.view.update(cx, |view, cx| view.replace_doc(doc, cx));
            }
            Err(refusal) => split.refusal = Some(format!("No preview: {refusal}.").into()),
        }
        self.sync_preview(cx);
        cx.notify();
    }

    fn sync_preview(&mut self, cx: &mut Context<Self>) {
        if !self.is_split() {
            return;
        }
        let row = self.cursor_row(cx);
        let Some(split) = self.split.as_mut() else {
            return;
        };
        if split.synced == Some(row) {
            return;
        }
        split.synced = Some(row);
        split.view.update(cx, |view, cx| view.reveal_line(row, cx));
    }

    pub(super) fn split_pane(&self, t: &Theme, window: &Window, cx: &App) -> Option<AnyElement> {
        let split = self.split.as_ref().filter(|_| self.is_split())?;
        let is_focused = split.view.read(cx).focus().is_focused(window);
        let note = split.refusal.clone().map(|note| {
            div()
                .id("editor-preview-note")
                .role(Role::Status)
                .aria_label(note.clone())
                .test_support()
                .flex_none()
                .px(t.spacing.xxl)
                .py(t.spacing.md)
                .text_color(t.colors.muted)
                .child(note)
        });
        let pane = div()
            .id("editor-preview")
            .test_support()
            .key_context(PREVIEW_CONTEXT)
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .border_l_2()
            .border_color(if is_focused {
                t.colors.accent
            } else {
                t.colors.line
            })
            .children(note)
            .child(split.view.clone());
        Some(pane.into_any_element())
    }

    pub(super) fn preview_keys(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &FocusPreview, window, cx| {
            view.focus_preview(window, cx);
        }))
        .on_action(cx.listener(|view, _: &LeavePreview, window, cx| {
            view.leave_preview(window, cx);
        }))
    }
}
