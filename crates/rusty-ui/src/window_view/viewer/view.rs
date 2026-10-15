use gpui_kit::base::TestSupportExt;
use gpui_kit::component::input::Editor;

use super::{CONTEXT, CloseViewer, SaveFile, Viewer};
use crate::prelude::*;
use crate::tokens::{self, Theme};
use crate::window_view::FerriteWindow;

fn header(t: &Theme, viewer: &Viewer) -> Div {
    div()
        .flex()
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
                .child(viewer.title.clone()),
        )
        .when(viewer.dirty, |d| {
            d.child(div().text_color(t.colors.mark).child("●"))
        })
        .child(div().flex_1())
        .when_some(viewer.message.clone(), |d, message| {
            d.child(
                div()
                    .id("viewer-message")
                    .test_support()
                    .text_color(t.colors.muted)
                    .text_size(t.typography.small)
                    .child(message),
            )
        })
        .child(
            div()
                .text_color(t.colors.faint)
                .text_size(t.typography.small)
                .child("Ctrl+F find · Ctrl+S save · Esc close"),
        )
}

fn body(t: &Theme, viewer: &Viewer) -> Div {
    match &viewer.editor {
        Some(editor) => div()
            .flex_1()
            .min_h_0()
            .child(Editor::new(editor).h_full().readonly(viewer.read_only)),
        None => div()
            .flex_1()
            .p(t.spacing.xxl)
            .text_color(t.colors.muted)
            .child("Esc closes."),
    }
}

impl FerriteWindow {
    pub(in crate::window_view) fn viewer_view(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let viewer = self.viewer.as_ref()?;
        let t = tokens::current(cx).clone();
        let save = cx.listener(|view, _: &SaveFile, _, cx| view.save_viewer(cx));
        let close = cx.listener(|view, _: &CloseViewer, window, cx| view.close_viewer(window, cx));
        Some(
            div()
                .id("viewer")
                .key_context(CONTEXT)
                .track_focus(&viewer.focus)
                .on_action(save)
                .on_action(close)
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .bg(t.colors.surface)
                .child(header(&t, viewer))
                .child(body(&t, viewer))
                .into_any_element(),
        )
    }
}
