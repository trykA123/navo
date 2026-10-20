use super::ViewOp;
use crate::prelude::*;
use crate::settings::Field;
use crate::window_view::FerriteWindow;
use crate::window_view::viewer::editor::EditorBody;

impl FerriteWindow {
    pub(super) fn toggle_split(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let can_toggle = self
            .viewer
            .as_ref()
            .and_then(|viewer| viewer.body.as_ref())
            .is_some_and(|body| {
                body.is_editing(cx)
                    && body
                        .parts(cx)
                        .is_some_and(|set| set.has(Field::MarkdownPreviewSplit))
            });
        if can_toggle {
            self.run_view(ViewOp::Toggle(Field::MarkdownPreviewSplit), window, cx);
        } else {
            cx.propagate();
        }
    }

    pub(super) fn leave_hidden_split(&self, window: &mut Window, cx: &mut App) {
        let Some(viewer) = &self.viewer else {
            return;
        };
        let Some(body) = &viewer.body else {
            return;
        };
        let is_hidden = body.parts(cx).is_some_and(|set| {
            set.has(Field::MarkdownPreviewSplit)
                && !viewer
                    .parts
                    .is_shown(set, Field::MarkdownPreviewSplit, &self.values)
        });
        let had_focus = body
            .as_any()
            .downcast_ref::<EditorBody>()
            .is_some_and(|editor| editor.has_preview_focus(window, cx));
        if is_hidden && had_focus {
            window.focus(&body.focus_handle(cx), cx);
        }
    }
}
