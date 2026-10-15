use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;

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
        .child(div().flex_1())
        .when_some(viewer.message.clone(), |d, message| {
            d.child(
                div()
                    .id("viewer-message")
                    .role(Role::Status)
                    .aria_label(message.clone())
                    .test_support()
                    .text_color(t.colors.muted)
                    .text_size(t.typography.small)
                    .child(message),
            )
        })
}

fn status(viewer: &Viewer) -> &'static str {
    match &viewer.opening {
        Some(opening) if opening.is_slow => "Opening… (Esc closes)",
        Some(_) => "Opening…",
        None => "Esc closes.",
    }
}

fn waiting(t: &Theme, viewer: &Viewer) -> impl IntoElement {
    div()
        .id("viewer-status")
        .role(Role::Status)
        .aria_label(status(viewer))
        .test_support()
        .flex_1()
        .p(t.spacing.xxl)
        .text_color(t.colors.muted)
        .child(status(viewer))
}

impl FerriteWindow {
    pub(in crate::window_view) fn viewer_view(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let viewer = self.viewer.as_ref()?;
        let t = tokens::current(cx).clone();
        let save = cx.listener(|view, _: &SaveFile, _, cx| view.save_viewer(cx));
        let close = cx.listener(|view, _: &CloseViewer, window, cx| view.close_viewer(window, cx));
        let root = div()
            .id("viewer")
            .key_context(CONTEXT)
            .track_focus(&viewer.focus)
            .on_action(save)
            .on_action(close);
        let root = Self::lister_actions(Self::slot_actions(root, cx), cx)
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .bg(t.colors.surface);
        let root = match &viewer.body {
            Some(body) => root.child(body.view()),
            None => root.child(header(&t, viewer)).child(waiting(&t, viewer)),
        };
        Some(root.into_any_element())
    }

    fn save_viewer(&mut self, cx: &mut Context<Self>) {
        if let Some(body) = self.viewer.as_ref().and_then(|viewer| viewer.body.as_ref()) {
            body.save(cx);
        }
    }
}
