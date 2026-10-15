mod body;
mod save;

use gpui_kit::base::TestSupportExt;
use gpui_kit::component::input::{Editor, EditorState, InputEvent};
use gpui_kit::{Role, Subscription, Task};

pub(super) use body::{EditorBody, EditorFile};

use super::FileStamp;
use super::body::Host;
use crate::format;
use crate::prelude::*;
use crate::tokens::{self, Theme};

const NOT_UTF8: &str = "Not valid UTF-8: opened read-only.";

pub(super) struct EditorView {
    host: Host,
    editor: Entity<EditorState>,
    editing: bool,
    can_edit: bool,
    crlf: bool,
    stamp: Option<FileStamp>,
    dirty: bool,
    revision: u64,
    overwrite: bool,
    saving: Option<Task<()>>,
    note: Option<SharedString>,
    _changes: Subscription,
}

impl EditorView {
    fn new(opened: EditorFile, host: Host, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let EditorFile { file, language } = opened;
        let text = file.text;
        let editor = cx.new(|cx| {
            let mut state = EditorState::new(window, cx)
                .language(language)
                .default_value(text);
            state.set_readonly(true, cx);
            state
        });
        let changes = cx.subscribe(&editor, |view, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                view.text_changed(cx);
            }
        });
        Self {
            host,
            editor,
            editing: false,
            can_edit: !file.read_only,
            crlf: file.crlf,
            stamp: file.stamp,
            dirty: false,
            revision: 0,
            overwrite: false,
            saving: None,
            note: file.read_only.then(|| NOT_UTF8.into()),
            _changes: changes,
        }
    }

    fn text_changed(&mut self, cx: &mut Context<Self>) {
        self.dirty = true;
        self.revision += 1;
        self.overwrite = false;
        cx.notify();
    }

    fn start_editing(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.can_edit {
            return false;
        }
        if !self.editing {
            self.editing = true;
            self.editor
                .update(cx, |state, cx| state.set_readonly(false, cx));
            cx.notify();
        }
        true
    }

    fn header(&self, t: &Theme) -> Div {
        let mode = if self.editing {
            "Editing"
        } else {
            "Text · read-only"
        };
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.md)
            .px(t.spacing.xl)
            .h(t.spacing.path_height)
            .border_b_1()
            .border_color(t.colors.line)
            .bg(t.colors.rail)
            .child(
                div()
                    .id("viewer-title")
                    .test_support()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.host.title.clone()),
            )
            .child(
                div()
                    .flex()
                    .gap(t.spacing.md)
                    .text_color(t.colors.muted)
                    .text_size(t.typography.small)
                    .child(mode)
                    .children(self.stamp.map(|stamp| format::size(stamp.len))),
            )
            .when(self.dirty, |d| {
                d.child(div().text_color(t.colors.mark).child("●"))
            })
            .child(div().flex_1())
            .when_some(self.note.clone(), |d, note| {
                d.child(
                    div()
                        .id("viewer-message")
                        .role(Role::Status)
                        .aria_label(note.clone())
                        .test_support()
                        .text_color(t.colors.muted)
                        .text_size(t.typography.small)
                        .child(note),
                )
            })
    }
}

impl Render for EditorView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .size_full()
            .child(self.header(&t))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(Editor::new(&self.editor).h_full().readonly(!self.editing)),
            )
    }
}
