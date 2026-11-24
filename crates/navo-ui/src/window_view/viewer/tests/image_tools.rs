//! Image tools in the viewer: the pixel grid past 800 % and the eyedropper that copies a pixel's colour.

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, MouseButton, Pixels, Point, TestAppContext};
use image::ImageFormat;

use super::image::{Rig, image_view, open_first, rig};
use super::menu::{click_with, run_item};
use super::{redraw, visible};
use crate::window_view::tests::press;
use crate::window_view::viewer::image::fixtures::encoded;

fn open_halves(cx: &mut TestAppContext) -> Rig {
    let rig = rig(cx, &[], &[("a.txt", encoded(ImageFormat::Png, 16, 16))]);
    open_first(cx, &rig);
    press(cx, rig.0, &rig.1, "ctrl-1");
    rig
}

fn zoom_in(cx: &mut TestAppContext, rig: &Rig, times: usize) {
    for _ in 0..times {
        press(cx, rig.0, &rig.1, "+");
    }
    redraw(cx, rig.0);
}

fn grid_shown(cx: &mut TestAppContext, window: AnyWindowHandle) -> bool {
    redraw(cx, window);
    visible(cx, window, "image-grid")
}

fn centre_of(cx: &mut TestAppContext, rig: &Rig, x: u32, y: u32) -> Point<Pixels> {
    redraw(cx, rig.0);
    let image = image_view(cx, &rig.1).expect("an image is open");
    image.read_with(cx, |image, _| image.pixel_centre(x, y))
}

fn left_click(cx: &mut TestAppContext, window: AnyWindowHandle, at: Point<Pixels>) {
    click_with(cx, window, (MouseButton::Left, at), true);
    click_with(cx, window, (MouseButton::Left, at), false);
    redraw(cx, window);
}

fn clipboard(cx: &mut TestAppContext) -> Option<String> {
    cx.read_from_clipboard().and_then(|item| item.text())
}

#[gpui_kit::test]
fn the_pixel_grid_shows_only_past_800_percent(cx: &mut TestAppContext) {
    let rig = open_halves(cx);
    assert!(!grid_shown(cx, rig.0), "no grid at 100 %");
    zoom_in(cx, &rig, 9);
    assert!(!grid_shown(cx, rig.0), "no grid at 745 %");
    zoom_in(cx, &rig, 1);
    assert!(grid_shown(cx, rig.0), "a grid at 931 %");
    press(cx, rig.0, &rig.1, "-");
    assert!(!grid_shown(cx, rig.0), "the grid goes again at 745 %");
}

#[gpui_kit::test]
fn an_eyedropper_click_copies_the_pixel_hex(cx: &mut TestAppContext) {
    let rig = open_halves(cx);
    zoom_in(cx, &rig, 4);
    let red = centre_of(cx, &rig, 2, 8);
    left_click(cx, rig.0, red);
    assert_eq!(
        clipboard(cx),
        None,
        "a click without the eyedropper copies nothing"
    );
    press(cx, rig.0, &rig.1, "e");
    assert!(visible(cx, rig.0, "image-colour-line"));
    let red = centre_of(cx, &rig, 2, 8);
    left_click(cx, rig.0, red);
    assert_eq!(clipboard(cx).as_deref(), Some("#DC1E1E"));
    let blue = centre_of(cx, &rig, 13, 8);
    left_click(cx, rig.0, blue);
    assert_eq!(clipboard(cx).as_deref(), Some("#1E1EDC"));
}

#[gpui_kit::test]
fn the_view_menu_turns_the_eyedropper_on_and_the_status_line_copies_rgb(cx: &mut TestAppContext) {
    let rig = open_halves(cx);
    zoom_in(cx, &rig, 4);
    press(cx, rig.0, &rig.1, "alt-v");
    run_item(cx, rig.0, "Eyedropper");
    redraw(cx, rig.0);
    assert!(visible(cx, rig.0, "image-colour-line"));
    let red = centre_of(cx, &rig, 2, 8);
    left_click(cx, rig.0, red);
    cx.update_window(rig.0, |_, window, cx| {
        window.click(("image-colour", 1usize), cx);
    })
    .expect("window");
    assert_eq!(clipboard(cx).as_deref(), Some("rgb(220, 30, 30)"));
}
