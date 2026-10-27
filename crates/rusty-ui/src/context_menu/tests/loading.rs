use gpui_kit::{TestAppContext, px};
use rusty_core::shell_menu::Submenu;

use super::flyout::send_targets;
use super::{SEND_TO, open_default};

fn opening_send_to(cx: &mut TestAppContext) -> super::Opened {
    let menu = open_default(cx);
    menu.type_text("send", cx);
    menu.press(&["right"], cx);
    assert_eq!(menu.levels(cx), 2);
    menu
}

#[gpui_kit::test]
fn the_loading_bar_and_row_wait_150_ms_before_they_show(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    menu.press(&["down", "right"], cx);
    assert_eq!(menu.loading_bar(1, cx), None);
    assert!(menu.rows_in(1, cx).is_empty());
    menu.wait(140, cx);
    assert_eq!(menu.loading_bar(1, cx), None);
    menu.wait(20, cx);
    assert_eq!(menu.rows_in(1, cx), ["Loading…"]);
    let bar = menu.loading_bar(1, cx).expect("loading bar");
    let panel = menu.level_bounds_in(1, cx);
    assert_eq!(bar.size.height, px(2.));
    assert!(bar.top() - panel.top() <= px(1.), "{bar:?} {panel:?}");
    assert!(bar.size.width > panel.size.width / 2., "{bar:?}");
}

#[gpui_kit::test]
fn the_loading_bar_goes_when_the_flyout_fills(cx: &mut TestAppContext) {
    let menu = opening_send_to(cx);
    menu.wait(200, cx);
    assert!(menu.loading_bar(1, cx).is_some());
    menu.menu.update(cx, |menu, cx| {
        menu.set_submenu(&[SEND_TO], send_targets(), cx);
    });
    menu.frame(cx);
    assert_eq!(menu.loading_bar(1, cx), None);
    assert_eq!(menu.rows_in(1, cx).len(), 3);
}

#[gpui_kit::test]
fn a_fast_load_never_shows_the_bar(cx: &mut TestAppContext) {
    let menu = opening_send_to(cx);
    menu.wait(100, cx);
    menu.menu.update(cx, |menu, cx| {
        menu.set_submenu(&[SEND_TO], send_targets(), cx);
    });
    menu.wait(200, cx);
    assert_eq!(menu.loading_bar(1, cx), None);
}

#[gpui_kit::test]
fn a_failed_load_drops_the_bar(cx: &mut TestAppContext) {
    let menu = opening_send_to(cx);
    menu.wait(200, cx);
    menu.menu.update(cx, |menu, cx| {
        menu.set_submenu(&[SEND_TO], Submenu::Failed, cx);
    });
    menu.frame(cx);
    assert_eq!(menu.loading_bar(1, cx), None);
}

#[gpui_kit::test]
fn the_bar_sweeps_once_every_1400_ms(cx: &mut TestAppContext) {
    let menu = opening_send_to(cx);
    menu.wait(150, cx);
    let start = menu.sweep(1, cx).expect("sweep").left();
    menu.wait(700, cx);
    let halfway = menu.sweep(1, cx).expect("sweep").left();
    assert!(halfway - start > px(20.), "{start:?} {halfway:?}");
    menu.wait(700, cx);
    let again = menu.sweep(1, cx).expect("sweep").left();
    assert!((again - start).abs() < px(1.), "{start:?} {again:?}");
}

#[gpui_kit::test]
fn with_reduced_motion_the_bar_stands_still_and_full(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let menu = opening_send_to(cx);
    menu.wait(200, cx);
    let bar = menu.loading_bar(1, cx).expect("loading bar");
    assert_eq!(menu.sweep(1, cx), Some(bar));
    menu.wait(700, cx);
    assert_eq!(menu.sweep(1, cx), Some(bar));
}
