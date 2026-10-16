use gpui_kit::component::input::{InputEvent, Position};
use gpui_kit::{ClipboardItem, Focusable, Subscription};

use super::EditorView;
use crate::prelude::*;
use crate::primitives::{TextFieldState, text_field_state};
use crate::window_view::viewer::search::bar::parse_line;

pub(super) struct GoToBar {
    pub input: Entity<TextFieldState>,
    pub error: Option<SharedString>,
    _enter: Subscription,
}

impl EditorView {
    pub(super) fn open_go_to(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(bar) = &self.go_to {
            let focus = bar.input.read(cx).focus_handle(cx);
            return window.focus(&focus, cx);
        }
        if let Some(text) = self.typed(cx) {
            self.find.last = Some(text);
        }
        self.find.bar = None;
        let input = text_field_state("Line number", window, cx);
        let enter = cx.subscribe_in(&input, window, |view, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                view.submit_go_to(window, cx);
            }
        });
        window.focus(&input.read(cx).focus_handle(cx), cx);
        self.go_to = Some(GoToBar {
            input,
            error: None,
            _enter: enter,
        });
        cx.notify();
    }

    pub(super) fn close_go_to(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.go_to.take().is_none() {
            return false;
        }
        window.focus(&self.editor.read(cx).focus_handle(cx), cx);
        cx.notify();
        true
    }

    fn submit_go_to(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(bar) = &mut self.go_to else {
            return;
        };
        let text = bar.input.read(cx).value().to_string();
        match parse_line(&text) {
            Ok(line) => {
                self.close_go_to(window, cx);
                let row = u32::try_from(line - 1).unwrap_or(u32::MAX);
                self.editor.update(cx, |state, cx| {
                    state.set_cursor_position(Position::new(row, 0), window, cx);
                });
            }
            Err(error) => {
                bar.error = Some(error.into());
                cx.notify();
            }
        }
    }

    pub(super) fn copy_path_line(&mut self, cx: &mut Context<Self>) {
        let line = self.cursor_row(cx) + 1;
        let text = format!("{}:{line}", self.host.location.as_str());
        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
        self.note = Some(format!("Copied {text}").into());
        cx.notify();
    }
}
