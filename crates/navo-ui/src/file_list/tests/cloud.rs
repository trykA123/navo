//! Cloud badge tests: a cloud placeholder row (a OneDrive online-only file) shows a quiet cloud
//! glyph with its spoken label; a plain row shows none, and the badge moves no column.

use std::sync::Arc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};
use navo_fs::{EntryKind, FsEntry};

use super::{entry, open_list, props};

/// Two files of the same name length: row 0 is online-only, row 1 is plain.
fn rows() -> Arc<[FsEntry]> {
    let mut cloud = entry("report.docx", EntryKind::File, 2_048);
    cloud.attributes.cloud_placeholder = true;
    vec![cloud, entry("budget.xlsx", EntryKind::File, 2_048)].into()
}

#[gpui_kit::test]
fn a_cloud_row_shows_the_badge_and_a_plain_row_does_not(cx: &mut TestAppContext) {
    let (window, _) = open_list(cx, props(rows()));
    let label = navo_l10n::tr("files-cloud-placeholder");
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let mut left = window.within("left");
        let badge = left.within(("row", 0usize)).find("cloud-badge");
        assert_eq!(badge.label(), Some(label.as_str()));
        assert!(
            left.within(("row", 1usize))
                .try_find("cloud-badge")
                .is_none()
        );
    })
    .expect("window");
}

#[gpui_kit::test]
fn the_badge_keeps_row_height_and_columns(cx: &mut TestAppContext) {
    let (window, _) = open_list(cx, props(rows()));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let mut left = window.within("left");
        let cloud = left.find(("row", 0usize)).bounds();
        let plain = left.find(("row", 1usize)).bounds();
        assert_eq!(cloud.size.height, plain.size.height);
        let mut size_x = |row: usize| {
            left.within(("row", row))
                .find(("cell", 2usize))
                .bounds()
                .origin
                .x
        };
        assert_eq!(size_x(0), size_x(1));
    })
    .expect("window");
}

#[gpui_kit::test]
fn a_cloud_row_says_it_is_online_only(cx: &mut TestAppContext) {
    let (window, _) = open_list(cx, props(rows()));
    let tail = format!(", {}", navo_l10n::tr("files-cloud-placeholder"));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let left = window.within("left");
        let spoken = |row: usize| left.find(("row", row)).label().map(str::to_owned);
        assert!(spoken(0).is_some_and(|words| words.ends_with(&tail)));
        assert!(spoken(1).is_some_and(|words| !words.ends_with(&tail)));
    })
    .expect("window");
}
