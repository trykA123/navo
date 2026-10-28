use gpui_kit::base::TestSupportExt;
use gpui_kit::{
    AnyElement, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    div,
};

use crate::tokens::Theme;

pub(super) fn split(relative: &str) -> (&str, &str) {
    relative
        .rsplit_once(['/', '\\'])
        .map_or((relative, ""), |(folder, name)| (name, folder))
}

pub(super) fn folder_header(t: &Theme) -> AnyElement {
    folder_cell(
        "find-folder-header",
        &rusty_l10n::t!("find-column-folder"),
        t,
    )
}

pub(super) fn folder_cell(id: &'static str, text: &str, t: &Theme) -> AnyElement {
    div()
        .id(id)
        .aria_label(text.to_owned())
        .test_support()
        .flex_1()
        .min_w_0()
        .truncate()
        .text_color(t.colors.muted)
        .child(text.to_owned())
        .into_any_element()
}
