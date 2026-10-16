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
