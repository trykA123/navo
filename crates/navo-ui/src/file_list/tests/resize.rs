use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, TestAppContext, point, px};

use super::{actions, open_list, props, sample_rows};
use crate::file_list::{FileListAction, SortBy};

const EXT: usize = 1;
const SIZE: usize = 2;

fn header_width(cx: &mut TestAppContext, window: AnyWindowHandle, column: usize) -> f32 {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let width = window
            .within("left")
            .find(("sort", column))
            .bounds()
            .size
            .width;
        f32::from(width)
    })
    .expect("window")
}

#[gpui_kit::test]
fn a_click_on_a_divider_does_not_sort(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    cx.update_window(window, |_, window, cx| {
        window.within("left").click(("column-handle", SIZE), cx);
    })
    .expect("window");
    let sorted = actions(&harness, cx)
        .into_iter()
        .any(|action| matches!(action, FileListAction::Sort(_)));
    assert!(!sorted);
}

#[gpui_kit::test]
fn dragging_a_divider_reports_the_new_width_once_on_release(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    let before = header_width(cx, window, EXT);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let from = window
            .within("left")
            .find(("column-handle", EXT))
            .bounds()
            .center();
        window.drag(from, point(from.x - px(30.), from.y), cx);
    })
    .expect("window");
    let resized: Vec<_> = actions(&harness, cx)
        .into_iter()
        .filter_map(|action| match action {
            FileListAction::ResizeColumn { by, width } => Some((by, f32::from(width))),
            _ => None,
        })
        .collect();
    assert_eq!(resized.len(), 1, "{resized:?}");
    assert_eq!(resized[0].0, SortBy::Ext);
    assert!(
        (resized[0].1 - (before + 30.)).abs() <= 1.,
        "{before} -> {resized:?}"
    );
}

#[gpui_kit::test]
fn a_drag_released_on_another_header_cell_clears_the_resize_without_sorting(
    cx: &mut TestAppContext,
) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    let before = header_width(cx, window, EXT);
    cx.update_window(window, |_, window, cx| {
        window
            .within("left")
            .drag_to(("column-handle", EXT), ("sort", 0usize), cx);
    })
    .expect("window");
    let sorted = actions(&harness, cx)
        .into_iter()
        .any(|action| matches!(action, FileListAction::Sort(_)));
    assert!(!sorted);
    let after = header_width(cx, window, EXT);
    assert!((after - before).abs() <= 1., "{before} -> {after}");
}

#[gpui_kit::test]
fn a_drag_released_outside_the_list_clears_the_resize_without_sorting(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    let before = header_width(cx, window, EXT);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let from = window
            .within("left")
            .find(("column-handle", EXT))
            .bounds()
            .center();
        window.drag(from, point(from.x, from.y + px(900.)), cx);
    })
    .expect("window");
    let sorted = actions(&harness, cx)
        .into_iter()
        .any(|action| matches!(action, FileListAction::Sort(_)));
    assert!(!sorted);
    let after = header_width(cx, window, EXT);
    assert!((after - before).abs() <= 1., "{before} -> {after}");
}

#[gpui_kit::test]
fn a_double_click_on_a_divider_stores_zero_to_fit_the_content(cx: &mut TestAppContext) {
    let (window, harness) = open_list(cx, props(sample_rows()));
    cx.update_window(window, |_, window, cx| {
        window
            .within("left")
            .double_click(("column-handle", SIZE), cx);
    })
    .expect("window");
    let resized: Vec<_> = actions(&harness, cx)
        .into_iter()
        .filter_map(|action| match action {
            FileListAction::ResizeColumn { by, width } => Some((by, f32::from(width))),
            _ => None,
        })
        .collect();
    assert_eq!(resized, vec![(SortBy::Size, 0.)]);
}
