use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext, px};

use super::{generated_rows, open_list, props};
use crate::tokens::{self, Density, Mode};

fn row_height(cx: &mut TestAppContext, density: Density) -> (gpui_kit::Pixels, u64) {
    let (window, harness) = open_list(cx, props(generated_rows(200)));
    cx.update(|cx| tokens::apply_with(Mode::Light, density, cx));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);
        window.find(("row", 0usize)).bounds().size.height
    })
    .map(|height| {
        let rows = harness.read_with(cx, |h, _| h.page_rows.last().copied().unwrap_or(0));
        (height, rows)
    })
    .expect("window")
}

#[gpui_kit::test]
fn normal_density_keeps_24_pixel_rows(cx: &mut TestAppContext) {
    assert_eq!(row_height(cx, Density::Normal).0, px(24.));
}

#[gpui_kit::test]
fn compact_rows_are_20_pixels_and_fit_more_on_a_page(cx: &mut TestAppContext) {
    let (height, rows) = row_height(cx, Density::Compact);
    assert_eq!(height, px(20.));
    assert!(rows > 18, "a compact page shows {rows} rows");
}

#[gpui_kit::test]
fn comfortable_rows_are_30_pixels(cx: &mut TestAppContext) {
    assert_eq!(row_height(cx, Density::Comfortable).0, px(30.));
}

fn first_visible_row(window: &mut gpui_kit::Window, cx: &mut gpui_kit::App) -> Option<usize> {
    window.render_frame(cx);
    (0..200usize).find(|index| {
        window
            .try_find(("row", *index))
            .is_some_and(|row| row.visible())
    })
}

#[gpui_kit::test]
fn a_density_change_keeps_the_same_top_row_and_the_cursor_in_view(cx: &mut TestAppContext) {
    let mut scrolled = props(generated_rows(200));
    scrolled.view = rusty_core::ViewState {
        cursor: 110,
        scroll: 100,
    };
    let (window, _) = open_list(cx, scrolled);
    let top = cx
        .update_window(window, |_, window, cx| first_visible_row(window, cx))
        .expect("window");
    assert_eq!(top, Some(100));
    for density in [Density::Comfortable, Density::Compact, Density::Normal] {
        cx.update(|cx| tokens::apply_with(Mode::Light, density, cx));
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(first_visible_row(window, cx), Some(100), "{density:?}");
            let cursor = window.find(("row", 110usize));
            assert!(
                cursor.visible(),
                "{density:?}: the cursor row is out of view"
            );
        })
        .expect("window");
    }
}
