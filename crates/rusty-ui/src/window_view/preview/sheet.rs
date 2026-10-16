use gpui_kit::base::TestSupportExt;
use gpui_kit::{ClickEvent, Role};

use super::Shown;
use super::file::FilePreview;
use super::header::{title_of, title_view};
use super::summary::{Sections, summary};
use crate::pane_chrome::PreviewButton;
use crate::prelude::*;
use crate::side_sheet::{SheetFrame, SideSheet, on_resize_keys};
use crate::tokens::{self, Theme};
use crate::window_view::FerriteWindow;

const WIDE_SHEET: f32 = 760.;
const ROW_STATS: f32 = 500.;
const SUMMARY_SHARE: f32 = 0.42;

fn note(t: &Theme, id: &'static str, text: SharedString) -> impl IntoElement + use<> {
    div()
        .id(id)
        .role(Role::Status)
        .aria_label(text.clone())
        .test_support()
        .p(t.spacing.xxl)
        .text_color(t.colors.muted)
        .child(text)
}

fn file_body(t: &Theme, file: &FilePreview) -> AnyElement {
    if let Some(body) = &file.body {
        return div()
            .id("preview-body")
            .test_support()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_hidden()
            .child(body.view())
            .into_any_element();
    }
    match &file.message {
        Some(message) => note(t, "preview-message", message.clone()).into_any_element(),
        None if file.is_loading() => {
            note(t, "preview-loading", "Loading…".into()).into_any_element()
        }
        None => note(t, "preview-message", "Nothing to show.".into()).into_any_element(),
    }
}

impl FerriteWindow {
    pub(in crate::window_view) fn preview_keys(
        &self,
        root: Div,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let t = tokens::current(cx).clone();
        let frame = self.sheet_frame(&t, window, cx);
        on_resize_keys(root, &self.preview.sheet, frame)
    }

    pub(in crate::window_view) fn preview_button(
        &self,
        pane: rusty_core::PaneId,
        cx: &mut Context<Self>,
    ) -> PreviewButton {
        PreviewButton::new(("preview-button", pane.get()), self.preview.is_open()).on_click(
            cx.listener(|this, _: &ClickEvent, window, cx| this.toggle_preview(window, cx)),
        )
    }

    pub(in crate::window_view) fn preview_layer(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.viewer.is_some() || !self.preview.sheet.read(cx).is_shown(cx) {
            return None;
        }
        let t = tokens::current(cx).clone();
        let frame = self.sheet_frame(&t, window, cx);
        let this = cx.weak_entity();
        let shown = self.preview.shown.as_ref();
        let is_folder = matches!(shown, Some(Shown::Folder(_)));
        let sheet = SideSheet::new("preview-sheet", &self.preview.sheet, frame)
            .label("Preview")
            .header(title_view(&t, title_of(&t, shown)))
            .close("ctrl-q", move |window, cx| {
                let _ = this.update(cx, |view, cx| view.toggle_preview(window, cx));
            })
            .body(self.preview_body(&t, frame, cx))
            .hint(&["ctrl-shift-q"], "Browse")
            .hint(&["enter", "f3"], "Open");
        let sheet = if is_folder {
            sheet.hint(&["escape"], "Stop scan")
        } else {
            sheet
        };
        let sheet = sheet.hint(&["[", "]"], "").hint(&["ctrl-q"], "Close");
        Some(
            div()
                .absolute()
                .top(t.spacing.xs)
                .bottom(t.spacing.xs)
                .left(t.spacing.md)
                .right(t.spacing.md)
                .child(sheet)
                .into_any_element(),
        )
    }

    fn preview_body(&self, t: &Theme, frame: SheetFrame, cx: &mut Context<Self>) -> AnyElement {
        match self.preview.shown.as_ref() {
            Some(Shown::File(file)) => file_body(t, file),
            Some(Shown::Folder(_)) => self.folder_body(t, frame, cx),
            Some(Shown::Empty) | None => {
                note(t, "preview-empty", "Nothing to preview here.".into()).into_any_element()
            }
        }
    }

    fn folder_body(&self, t: &Theme, frame: SheetFrame, cx: &mut Context<Self>) -> AnyElement {
        let width = self.preview.sheet.read(cx).width(&frame);
        let is_wide = width >= px(WIDE_SHEET);
        let summary_width = if is_wide {
            width * SUMMARY_SHARE
        } else {
            width
        };
        let Some(folder) = self.preview.folder() else {
            return div().into_any_element();
        };
        let sections = Sections {
            stats: Self::stats(folder, summary_width >= px(ROW_STATS), t, cx),
            profile: folder.scan.profile.clone(),
            scroll: folder.summary_scroll.clone(),
        };
        let contents = self.contents_view(t, cx);
        let rows = folder.contents.len().max(1) as f32;
        let tree_height = px(28. + 8. + 4.) + px(24.) * rows;
        let region = || div().flex().min_h_0().min_w_0().overflow_hidden();
        let (tree, rest) = if is_wide {
            (
                region().flex_grow(1.4).flex_basis(px(0.)),
                region().flex_grow(1.).flex_basis(px(0.)),
            )
        } else {
            (
                region().flex_grow(3.).flex_basis(px(0.)).max_h(tree_height),
                region().flex_grow(2.).flex_basis(px(0.)).min_h(px(180.)),
            )
        };
        div()
            .id("preview-folder")
            .test_support()
            .flex()
            .when(!is_wide, |body| body.flex_col())
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(tree.children(contents))
            .child(rest.child(summary(t, sections)))
            .into_any_element()
    }
}
