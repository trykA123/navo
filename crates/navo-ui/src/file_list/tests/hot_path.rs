//! Pins the file list's per-row text and filter highlights, so the repaint hot path can reuse
//! its buffers without changing what a row shows.

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};

use super::{FileListProps, open_list, props, sample_rows};
use crate::file_list::row::found_range;
use crate::name_match::{Needle, Scratch};

/// Reads the name label of each sample row with the given filter text.
fn name_labels(cx: &mut TestAppContext, filter: Option<&str>) -> Vec<String> {
    let filtered = FileListProps {
        filter: filter.map(|text| text.to_owned().into()),
        ..props(sample_rows())
    };
    let (window, _) = open_list(cx, filtered);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let mut left = window.within("left");
        (0..6usize)
            .map(|index| {
                let name = left.within(("row", index)).find("find-name");
                name.label().unwrap_or_default().to_owned()
            })
            .collect()
    })
    .expect("window")
}

#[gpui_kit::test]
fn rows_show_the_same_names_with_and_without_a_filter(cx: &mut TestAppContext) {
    let expected = ["[src]", "[tests]", "Cargo", "layout", "docs", ".env"];
    assert_eq!(name_labels(cx, None), expected);
    assert_eq!(name_labels(cx, Some("o")), expected);
}

#[test]
fn a_filter_highlights_the_first_match_shifted_past_a_folder_bracket() {
    let needle = Needle::parse("o");
    let mut scratch = Scratch::default();
    let find = |full, len, is_folder, scratch: &mut Scratch| {
        found_range(&needle, full, (len, is_folder), scratch)
    };
    assert_eq!(find("Cargo.toml", 5, false, &mut scratch), Some(4..5));
    assert_eq!(find("docs", 6, true, &mut scratch), Some(2..3));
    assert_eq!(find("src", 5, true, &mut scratch), None);
}

#[test]
fn a_match_past_the_shown_text_is_no_highlight() {
    let needle = Needle::parse("toml");
    let found = found_range(&needle, "Cargo.toml", (5, false), &mut Scratch::default());
    assert_eq!(found, None);
}

#[test]
fn one_scratch_reused_across_rows_finds_what_a_fresh_one_finds() {
    let needle = Needle::parse("ÄB;x");
    let names = ["äbc", "Straße", "xÄbx", "plain", "ÄÄB", ""];
    let mut shared = Scratch::default();
    for name in names {
        let shown = (name.len(), false);
        let fresh = found_range(&needle, name, shown, &mut Scratch::default());
        let reused = found_range(&needle, name, shown, &mut shared);
        assert_eq!(reused, fresh, "{name}");
    }
}
