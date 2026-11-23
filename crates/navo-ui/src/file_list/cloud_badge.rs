//! Cloud badge: a quiet cloud glyph after the name of a cloud placeholder row (a OneDrive
//! online-only file). It sits inside the name cell, so it never moves a column or the row height.

use gpui_kit::base::TestSupportExt;
use gpui_kit::{
    InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled, div, px,
};
use navo_fs::FsEntry;

use crate::primitives::{IconName, Tooltip, icon};
use crate::tokens::Theme;

/// Glyph size below the row icon's, so the badge reads as a note on the name.
const GLYPH_SHRINK: f32 = 3.;

/// Builds the badge for `entry` in theme `t`; `None` when the file is not a cloud placeholder.
/// The tooltip and the accessible label both say the file opens after a download.
pub(super) fn badge(t: &Theme, entry: &FsEntry) -> Option<impl IntoElement> {
    if !entry.attributes.cloud_placeholder {
        return None;
    }
    let tip = navo_l10n::tr("files-cloud-placeholder");
    Some(
        div()
            .id("cloud-badge")
            .aria_label(tip.clone())
            .test_support()
            .flex_none()
            .flex()
            .items_center()
            .child(icon(
                IconName::Cloud,
                t.spacing.icon - px(GLYPH_SHRINK),
                t.colors.muted,
            ))
            .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx)),
    )
}
