use gpui_kit::TestAppContext;

use super::super::{fake, settle, two_panes};
use super::{Rig, rig, rig_on, rig_with, windows_menu};

fn ready(rig: &Rig) {
    *rig.shell.answer.borrow_mut() = Some(Ok(windows_menu()));
}

fn nothing_built(rig: &Rig) -> bool {
    rig.shell.opened.borrow().is_empty()
        && rig.shell.backgrounds.borrow().is_empty()
        && rig.shell.live.get() == 0
}

#[gpui_kit::test]
fn a_cursor_resting_on_a_file_builds_no_menu(cx: &mut TestAppContext) {
    let rig = rig(cx);
    ready(&rig);
    rig.press(cx, "down");
    rig.wait(cx, 400);
    rig.wait(cx, 10_000);
    assert!(nothing_built(&rig));
}

#[gpui_kit::test]
fn twenty_cursor_steps_build_no_menu(cx: &mut TestAppContext) {
    let rig = rig(cx);
    ready(&rig);
    for step in 0..20 {
        rig.press(cx, if step % 2 == 0 { "down" } else { "up" });
        rig.wait(cx, 350);
    }
    assert!(nothing_built(&rig));
}

#[gpui_kit::test]
fn marking_everything_builds_no_menu(cx: &mut TestAppContext) {
    let rig = rig(cx);
    ready(&rig);
    rig.press(cx, "ctrl-a");
    rig.wait(cx, 400);
    assert!(nothing_built(&rig));
}

#[gpui_kit::test]
fn an_empty_folder_at_rest_builds_no_background_menu(cx: &mut TestAppContext) {
    let fs = fake();
    fs.add_folder("home/empty");
    let state = two_panes(&fs);
    let rig = rig_on(cx, true, (fs, state));
    ready(&rig);
    rig.press(cx, "down");
    rig.press(cx, "enter");
    settle(cx, &rig.view);
    rig.wait(cx, 400);
    assert!(nothing_built(&rig));
    rig.press(cx, "menu");
    rig.wait(cx, 50);
    assert_eq!(
        *rig.shell.backgrounds.borrow(),
        [rig.location("home/empty")]
    );
    assert!(rig.is_open(cx));
}

#[gpui_kit::test]
fn the_menu_key_builds_one_menu_after_a_rest(cx: &mut TestAppContext) {
    let rig = rig(cx);
    ready(&rig);
    rig.press(cx, "down");
    rig.wait(cx, 400);
    rig.press(cx, "menu");
    rig.wait(cx, 50);
    assert_eq!(
        *rig.shell.opened.borrow(),
        [vec![rig.location("home/a.txt")]]
    );
    assert!(rig.rows(cx).iter().any(|row| row == "7-Zip"));
}

#[gpui_kit::test]
fn shift_f10_builds_one_menu_after_a_rest(cx: &mut TestAppContext) {
    let rig = rig(cx);
    ready(&rig);
    rig.press(cx, "down");
    rig.wait(cx, 400);
    rig.press(cx, "shift-f10");
    rig.wait(cx, 50);
    assert_eq!(rig.shell.opened.borrow().len(), 1);
    assert!(rig.is_open(cx));
}

#[gpui_kit::test]
fn a_long_right_press_builds_one_menu(cx: &mut TestAppContext) {
    let rig = rig(cx);
    ready(&rig);
    rig.press(cx, "down");
    rig.wait(cx, 400);
    rig.right_button(cx, 1, true);
    rig.wait(cx, 560);
    assert!(rig.is_open(cx));
    assert_eq!(rig.shell.opened.borrow().len(), 1);
}

#[gpui_kit::test]
fn more_than_100_marked_items_still_open_a_menu_on_demand(cx: &mut TestAppContext) {
    let names: Vec<String> = (0..101).map(|at| format!("home/f{at:03}.txt")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let rig = rig_with(cx, true, &names);
    ready(&rig);
    rig.press(cx, "ctrl-a");
    rig.wait(cx, 400);
    assert!(nothing_built(&rig));
    rig.press(cx, "menu");
    rig.wait(cx, 50);
    assert_eq!(rig.shell.opened.borrow().len(), 1);
}
