use gpui_kit::TestAppContext;
use rusty_core::shell_menu::{Group, ItemKind, MenuItem, Submenu};

use super::{open_by_key, shell_item, windows_menu};

#[gpui_kit::test]
fn the_shown_menu_prefills_its_submenus_without_asking_for_them(cx: &mut TestAppContext) {
    let rig = open_by_key(cx);
    rig.answer(cx, Ok(windows_menu()));
    assert_eq!(*rig.shell.prefilled.borrow(), [vec![2]]);
    assert!(rig.shell.asked.borrow().is_empty());
}

#[gpui_kit::test]
fn opening_a_submenu_still_prefilling_asks_for_it_now(cx: &mut TestAppContext) {
    let rig = open_by_key(cx);
    rig.answer(cx, Ok(windows_menu()));
    rig.type_text(cx, "send");
    rig.press(cx, "right");
    assert_eq!(*rig.shell.asked.borrow(), [vec![2]]);
}

#[gpui_kit::test]
fn a_prefilled_submenu_opens_without_asking_again(cx: &mut TestAppContext) {
    let rig = open_by_key(cx);
    let documents = Submenu::Filled(vec![shell_item(50, "Documents", Group::Share)]);
    rig.shell.fills.borrow_mut().insert(vec![2], documents);
    rig.answer(cx, Ok(windows_menu()));
    rig.type_text(cx, "send");
    rig.press(cx, "right");
    assert_eq!(rig.rows_in(cx, 1), ["Documents"]);
    assert!(rig.shell.asked.borrow().is_empty());
}

#[gpui_kit::test]
fn opening_a_submenu_whose_prefill_failed_asks_for_it_again(cx: &mut TestAppContext) {
    let rig = open_by_key(cx);
    rig.shell
        .fills
        .borrow_mut()
        .insert(vec![2], Submenu::Failed);
    rig.answer(cx, Ok(windows_menu()));
    rig.wait(cx, 50);
    rig.type_text(cx, "send");
    rig.press(cx, "right");
    assert_eq!(*rig.shell.asked.borrow(), [vec![2]]);
}

#[gpui_kit::test]
fn a_menu_closed_before_the_shell_answers_prefills_nothing(cx: &mut TestAppContext) {
    let rig = open_by_key(cx);
    rig.press(cx, "escape");
    rig.answer(cx, Ok(windows_menu()));
    assert!(rig.shell.prefilled.borrow().is_empty());
}

#[gpui_kit::test]
fn closing_the_menu_ends_the_session_with_its_prefills(cx: &mut TestAppContext) {
    let rig = open_by_key(cx);
    rig.answer(cx, Ok(windows_menu()));
    assert_eq!(rig.shell.live.get(), 1);
    rig.press(cx, "escape");
    rig.wait(cx, 50);
    assert_eq!(rig.shell.live.get(), 0);
}

#[gpui_kit::test]
fn deeper_submenus_wait_for_their_parent_to_fill(cx: &mut TestAppContext) {
    let rig = open_by_key(cx);
    rig.answer(cx, Ok(windows_menu()));
    let nested = MenuItem {
        kind: ItemKind::Submenu(Submenu::Lazy),
        ..shell_item(51, "More", Group::Share)
    };
    rig.shell
        .fills
        .borrow_mut()
        .insert(vec![2], Submenu::Filled(vec![nested]));
    rig.wait(cx, 50);
    assert_eq!(*rig.shell.prefilled.borrow(), [vec![2], vec![2, 0]]);
}
