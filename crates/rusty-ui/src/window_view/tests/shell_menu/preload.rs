use gpui_kit::TestAppContext;
use rusty_core::shell_menu::ShellMenu;

use super::super::{fake, settle, two_panes};
use super::{Rig, rig, rig_on, rig_with, windows_menu};

fn ready(rig: &Rig) {
    *rig.shell.answer.borrow_mut() = Some(Ok(windows_menu()));
}

fn resting_on_a_file(cx: &mut TestAppContext) -> Rig {
    let rig = rig(cx);
    ready(&rig);
    rig.press(cx, "down");
    rig.wait(cx, 350);
    rig
}

#[gpui_kit::test]
fn a_cursor_resting_300_ms_prepares_its_menu(cx: &mut TestAppContext) {
    let rig = rig(cx);
    ready(&rig);
    rig.press(cx, "down");
    rig.wait(cx, 250);
    assert!(rig.shell.opened.borrow().is_empty());
    rig.wait(cx, 100);
    assert_eq!(
        *rig.shell.opened.borrow(),
        [vec![rig.location("home/a.txt")]]
    );
    assert!(!rig.is_open(cx));
}

#[gpui_kit::test]
fn the_place_seen_at_start_prepares_nothing(cx: &mut TestAppContext) {
    let rig = rig(cx);
    ready(&rig);
    rig.wait(cx, 400);
    assert!(rig.shell.opened.borrow().is_empty());
}

#[gpui_kit::test]
fn the_menu_key_opens_the_prepared_menu(cx: &mut TestAppContext) {
    let rig = resting_on_a_file(cx);
    rig.press(cx, "menu");
    rig.wait(cx, 50);
    assert_eq!(rig.shell.opened.borrow().len(), 1);
    assert!(rig.rows(cx).iter().any(|row| row == "7-Zip"));
}

#[gpui_kit::test]
fn shift_f10_opens_the_prepared_menu(cx: &mut TestAppContext) {
    let rig = resting_on_a_file(cx);
    rig.press(cx, "shift-f10");
    rig.wait(cx, 50);
    assert_eq!(rig.shell.opened.borrow().len(), 1);
    assert!(rig.is_open(cx));
}

#[gpui_kit::test]
fn a_long_right_press_on_the_resting_row_opens_the_prepared_menu(cx: &mut TestAppContext) {
    let rig = resting_on_a_file(cx);
    rig.right_button(cx, 1, true);
    rig.wait(cx, 560);
    assert!(rig.is_open(cx));
    assert_eq!(rig.shell.opened.borrow().len(), 1);
}

#[gpui_kit::test]
fn moving_the_cursor_drops_the_prepared_menu(cx: &mut TestAppContext) {
    let rig = resting_on_a_file(cx);
    assert_eq!(rig.shell.live.get(), 1);
    rig.press(cx, "up");
    assert_eq!(rig.shell.live.get(), 0);
}

#[gpui_kit::test]
fn marking_a_row_drops_the_prepared_menu(cx: &mut TestAppContext) {
    let rig = resting_on_a_file(cx);
    rig.press(cx, "space");
    assert_eq!(rig.shell.live.get(), 0);
}

#[gpui_kit::test]
fn only_one_prepared_menu_is_kept(cx: &mut TestAppContext) {
    let rig = resting_on_a_file(cx);
    rig.press(cx, "up");
    rig.wait(cx, 350);
    assert_eq!(rig.shell.opened.borrow().len(), 2);
    assert_eq!(rig.shell.live.get(), 1);
}

#[gpui_kit::test]
fn a_prepared_menu_is_dropped_after_10_s(cx: &mut TestAppContext) {
    let rig = resting_on_a_file(cx);
    rig.wait(cx, 9_000);
    assert_eq!(rig.shell.live.get(), 1);
    rig.wait(cx, 1_100);
    assert_eq!(rig.shell.live.get(), 0);
    rig.press(cx, "menu");
    assert_eq!(rig.shell.opened.borrow().len(), 2);
}

#[gpui_kit::test]
fn an_empty_folder_prepares_its_background_menu(cx: &mut TestAppContext) {
    let fs = fake();
    fs.add_folder("home/empty");
    let state = two_panes(&fs);
    let rig = rig_on(cx, true, (fs, state));
    ready(&rig);
    rig.press(cx, "down");
    rig.press(cx, "enter");
    settle(cx, &rig.view);
    rig.wait(cx, 350);
    let empty = rig.location("home/empty");
    assert_eq!(
        *rig.shell.backgrounds.borrow(),
        std::slice::from_ref(&empty)
    );
    rig.press(cx, "menu");
    rig.wait(cx, 50);
    assert_eq!(*rig.shell.backgrounds.borrow(), [empty]);
    assert!(rig.is_open(cx));
}

#[gpui_kit::test]
fn while_a_replaced_shell_thread_runs_nothing_is_prepared(cx: &mut TestAppContext) {
    let rig = rig(cx);
    ready(&rig);
    rig.shell.is_read_ahead_blocked.set(true);
    rig.press(cx, "down");
    rig.wait(cx, 350);
    assert!(rig.shell.opened.borrow().is_empty());
}

#[gpui_kit::test]
fn a_menu_prepared_before_its_shell_thread_was_replaced_opens_afresh(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, "down");
    rig.wait(cx, 350);
    rig.shell.replaced.set(1);
    ready(&rig);
    rig.press(cx, "menu");
    rig.wait(cx, 50);
    assert_eq!(rig.shell.opened.borrow().len(), 2);
    assert!(rig.rows(cx).iter().any(|row| row == "7-Zip"));
}

#[gpui_kit::test]
fn a_prepared_menu_that_failed_opens_afresh(cx: &mut TestAppContext) {
    let rig = rig(cx);
    *rig.shell.answer.borrow_mut() = Some(Err("stopped".to_owned()));
    rig.press(cx, "down");
    rig.wait(cx, 350);
    ready(&rig);
    rig.press(cx, "menu");
    rig.wait(cx, 50);
    assert_eq!(rig.shell.opened.borrow().len(), 2);
    assert!(rig.rows(cx).iter().any(|row| row == "7-Zip"));
}

#[gpui_kit::test]
fn a_prepared_menu_that_answered_keeps_its_answer(cx: &mut TestAppContext) {
    let rig = resting_on_a_file(cx);
    *rig.shell.answer.borrow_mut() = Some(Ok(ShellMenu::default()));
    rig.press(cx, "menu");
    rig.wait(cx, 50);
    assert_eq!(rig.shell.opened.borrow().len(), 1);
    assert!(rig.rows(cx).iter().any(|row| row == "7-Zip"));
}

#[gpui_kit::test]
fn marking_everything_in_a_small_folder_prepares_its_menu(cx: &mut TestAppContext) {
    let rig = rig(cx);
    ready(&rig);
    rig.press(cx, "ctrl-a");
    rig.wait(cx, 350);
    assert_eq!(rig.shell.opened.borrow().len(), 1);
}

#[gpui_kit::test]
fn more_than_100_marked_items_prepare_nothing(cx: &mut TestAppContext) {
    let names: Vec<String> = (0..101).map(|at| format!("home/f{at:03}.txt")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let rig = rig_with(cx, true, &names);
    ready(&rig);
    rig.press(cx, "ctrl-a");
    rig.wait(cx, 350);
    assert!(rig.shell.opened.borrow().is_empty());
}
