use gpui_kit::base::TestSupportExt;
use gpui_kit::component::input::Editor;

use super::{CONTEXT, CloseViewer, SaveFile, ShowHex, ShowText, Viewer};
use crate::format;
use crate::prelude::*;
use crate::tokens::{self, Theme};
use crate::window_view::FerriteWindow;

fn facts(t: &Theme, viewer: &Viewer) -> Div {
    let mode = if viewer.read_only {
        "Text · read-only"
    } else {
        "Text"
    };
    div()
        .flex()
        .gap(t.spacing.md)
        .text_color(t.colors.muted)
        .text_size(t.typography.small)
        .child(mode)
        .children(viewer.stamp.map(|stamp| format::size(stamp.len)))
}

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
        .child(facts(t, viewer))
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
                .child("Alt+3 hex · Ctrl+F find · Ctrl+S save · Esc close"),
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
        let text = cx.listener(|view, _: &ShowText, window, cx| view.show_text(window, cx));
        let hex = cx.listener(|view, _: &ShowHex, window, cx| view.show_hex(window, cx));
        let root = div()
            .id("viewer")
            .key_context(CONTEXT)
            .track_focus(&viewer.focus)
            .on_action(save)
            .on_action(close)
            .on_action(text)
            .on_action(hex)
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .bg(t.colors.surface);
        let root = match &viewer.paged {
            Some(paged) => root.child(paged.clone()),
            None => root.child(header(&t, viewer)).child(body(&t, viewer)),
        };
        Some(root.into_any_element())
    }
}
