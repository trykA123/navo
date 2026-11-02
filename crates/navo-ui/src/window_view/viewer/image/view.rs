use gpui_kit::Role;
use gpui_kit::Stateful;
use gpui_kit::base::{ObservedElement, TestSupportExt};

use super::ImageView;
use crate::format;
use crate::prelude::*;
use crate::primitives::{IconButton, IconName};
use crate::tokens::{self, Theme};
use crate::window_view::viewer::view_menu::view_button;
use crate::window_view::viewer::{
    ActualSize, CopyImage, FitImage, IMAGE_CONTEXT, RowDown, RowUp, ScrollLeft, ScrollRight,
    ToggleInfo, ZoomIn, ZoomOut,
};

const NO_EXIF: &str = "No EXIF data in this file.";

fn small(t: &Theme, color: Hsla) -> Div {
    div()
        .flex_none()
        .text_size(t.typography.small)
        .text_color(color)
}

fn labelled(t: &Theme, id: &'static str, text: SharedString) -> ObservedElement<Stateful<Div>> {
    small(t, t.colors.muted)
        .id(id)
        .aria_label(text.clone())
        .test_support()
        .child(text)
}

fn exif_row(t: &Theme, index: usize, label: &'static str, value: &str) -> impl IntoElement {
    div()
        .id(("image-exif", index))
        .aria_label(format!("{label}: {value}"))
        .test_support()
        .flex()
        .flex_col()
        .gap(t.spacing.xxs)
        .child(small(t, t.colors.muted).child(label))
        .child(div().text_color(t.colors.text).child(value.to_owned()))
}

impl ImageView {
    fn meta(&self) -> SharedString {
        let picture = &self.picture;
        format!(
            "{} × {} · {} · {}",
            picture.width,
            picture.height,
            picture.format,
            format::size(picture.bytes)
        )
        .into()
    }

    fn header(&self, t: &Theme) -> Div {
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
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.title.clone()),
            )
            .child(labelled(t, "image-meta", self.meta()))
            .child(div().flex_1())
            .children(
                self.note
                    .clone()
                    .map(|note| labelled(t, "image-note", note).role(Role::Status)),
            )
            .child(
                labelled(t, "image-zoom", self.zoom_label().into())
                    .role(Role::Status)
                    .font_family(t.typography.numbers_font.clone())
                    .font_features(t.typography.numbers_font_features.clone()),
            )
            .child(
                IconButton::new("image-info-toggle", IconName::Info, "Details (I)")
                    .selected(self.info)
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(ToggleInfo), cx)),
            )
            .child(view_button())
    }

    fn info_panel(&self, t: &Theme) -> impl IntoElement {
        let rows = self
            .picture
            .exif
            .iter()
            .enumerate()
            .map(|(index, entry)| exif_row(t, index, entry.label, &entry.value));
        div()
            .id("image-info")
            .test_support()
            .flex()
            .flex_col()
            .flex_none()
            .gap(t.spacing.lg)
            .w(t.spacing.rail_width)
            .p(t.spacing.xl)
            .border_l_1()
            .border_color(t.colors.line)
            .bg(t.colors.rail)
            .overflow_y_scroll()
            .child(
                small(t, t.colors.muted)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Details"),
            )
            .children(rows)
            .when(self.picture.exif.is_empty(), |panel| {
                panel.child(small(t, t.colors.muted).child(NO_EXIF))
            })
    }

    fn view_keys(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &FitImage, _, cx| view.fit(cx)))
            .on_action(cx.listener(|view, _: &CopyImage, _, cx| view.copy_image(cx)))
            .on_action(cx.listener(|view, _: &ActualSize, _, cx| view.actual_size(cx)))
            .on_action(cx.listener(|view, _: &ZoomIn, _, cx| view.zoom_step(true, None, cx)))
            .on_action(cx.listener(|view, _: &ZoomOut, _, cx| view.zoom_step(false, None, cx)))
            .on_action(cx.listener(|view, _: &ScrollLeft, _, cx| view.pan_step(1., 0., cx)))
            .on_action(cx.listener(|view, _: &ScrollRight, _, cx| view.pan_step(-1., 0., cx)))
            .on_action(cx.listener(|view, _: &RowUp, _, cx| view.pan_step(0., 1., cx)))
            .on_action(cx.listener(|view, _: &RowDown, _, cx| view.pan_step(0., -1., cx)))
    }
}

impl Render for ImageView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let viewport = self.viewport();
        self.zoom.clamp(&viewport);
        let root = div()
            .id("image")
            .key_context(IMAGE_CONTEXT)
            .track_focus(&self.focus);
        Self::view_keys(root, cx)
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .size_full()
            .bg(t.colors.surface)
            .child(self.header(&t))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.stage(&t, cx))
                    .when(self.info, |row| row.child(self.info_panel(&t))),
            )
    }
}
