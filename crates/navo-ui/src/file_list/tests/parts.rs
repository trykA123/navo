use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    Context, IntoElement, ParentElement, Render, Styled, TestAppContext, Window, div, px,
};

use super::{open_list, props, sample_rows};
use crate::file_list::row::name_text;
use crate::file_list::{FileList, FileListProps, ListParts, SortBy, SortKey};
use crate::test_window::open;

struct Parted(FileListProps, ListParts);

impl Render for Parted {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let list = FileList::new("left", self.0.clone()).parts(self.1);
        div()
            .size_full()
            .child(div().h(px(480.)).flex().flex_col().child(list))
    }
}

fn parted(cx: &mut TestAppContext, parts: ListParts, sort: SortKey) -> gpui_kit::AnyWindowHandle {
    let mut shown = props(sample_rows());
    shown.sort = sort;
    let (window, _) = open(cx, move |_, _| Parted(shown, parts));
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    window
}

fn headers(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle) -> [bool; 4] {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let list = window.within("left");
        [0usize, 1, 2, 3].map(|index| list.try_find(("sort", index)).is_some())
    })
    .expect("window")
}

fn has_icon(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle) -> bool {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let mut list = window.within("left");
        let row = list.within(("row", 2usize));
        row.try_find("row-icon").is_some()
    })
    .expect("window")
}

#[gpui_kit::test]
fn every_column_and_icon_shows_by_default(cx: &mut TestAppContext) {
    let (window, _) = open_list(cx, props(sample_rows()));
    assert_eq!(headers(cx, window), [true; 4]);
    assert!(has_icon(cx, window));
}

#[gpui_kit::test]
fn a_hidden_column_loses_its_header_and_name_stays(cx: &mut TestAppContext) {
    let parts = ListParts {
        ext: false,
        size: false,
        ..ListParts::default()
    };
    let window = parted(cx, parts, SortKey::default());
    assert_eq!(headers(cx, window), [true, false, false, true]);
}

#[gpui_kit::test]
fn hidden_icons_leave_the_rows_without_them(cx: &mut TestAppContext) {
    let parts = ListParts {
        icons: false,
        ..ListParts::default()
    };
    let window = parted(cx, parts, SortKey::default());
    assert!(!has_icon(cx, window));
}

#[gpui_kit::test]
fn sorting_by_a_hidden_column_shows_the_arrow_on_name_with_the_column_named(
    cx: &mut TestAppContext,
) {
    let parts = ListParts {
        size: false,
        ..ListParts::default()
    };
    let sort = SortKey {
        by: SortBy::Size,
        descending: true,
    };
    let window = parted(cx, parts, sort);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let mut list = window.within("left");
        let name = list.within(("sort", 0usize));
        assert_eq!(name.find("sort-hidden").label(), Some("Sorted by Size"));
    })
    .expect("window");
}

#[gpui_kit::test]
fn sorting_by_a_shown_column_adds_no_note(cx: &mut TestAppContext) {
    let sort = SortKey {
        by: SortBy::Size,
        descending: false,
    };
    let window = parted(cx, ListParts::default(), sort);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.within("left").try_find("sort-hidden").is_none());
    })
    .expect("window");
}

#[test]
fn names_carry_their_extension_once_the_ext_column_is_hidden() {
    assert_eq!(name_text(("report", "txt"), false, true), "report");
    assert_eq!(name_text(("report", "txt"), false, false), "report.txt");
    assert_eq!(name_text(("Makefile", ""), false, false), "Makefile");
    assert_eq!(name_text(("src", ""), true, false), "[src]");
}
