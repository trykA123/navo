//! The confirm dialog's summary: a first line beside the mode label, cut to fit, and an optional
//! second line that wraps whole below it.

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Div, InteractiveElement, ParentElement, Role, StatefulInteractiveElement, Styled, div,
};

use super::{ConfirmProps, label};
use crate::tokens::Theme;

/// The summary: its first line beside the mode label, cut to fit; text after a line break
/// wraps whole on its own line below.
pub(super) fn summary_row(t: &Theme, props: &ConfirmProps) -> Div {
    let (first, detail) = match props.summary.split_once('\n') {
        Some((first, detail)) => (first.to_owned().into(), Some(detail.to_owned())),
        None => (props.summary.clone(), None),
    };
    let line = div()
        .flex()
        .items_center()
        .justify_between()
        .gap(t.spacing.md)
        .child(label("summary", first).text_color(t.colors.muted))
        .when_some(props.mode.clone(), |row, mode| {
            row.child(
                label("mode", mode)
                    .flex_none()
                    .px(t.spacing.sm)
                    .py(t.spacing.xxs)
                    .rounded(t.radii.sm)
                    .border_1()
                    .border_color(t.colors.line)
                    .text_size(t.typography.small)
                    .text_color(t.colors.muted),
            )
        });
    let detail = detail.map(|text| {
        div()
            .id("summary-detail")
            .role(Role::Label)
            .aria_label(text.clone())
            .test_support()
            .text_color(t.colors.muted)
            .child(text)
    });
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.xxs)
        .child(line)
        .children(detail)
}
