use std::sync::Arc;

use gpui_kit::{TestAppContext, point, px};
use rusty_core::shell_menu::{Group, Icon, MenuItem, ShellMenu, Submenu};

use super::{SEND_TO, open_default, open_menu_with, shell};
use crate::context_menu::icons::{CACHED_ICONS, IconCache, ShellIcons};

fn icon(seed: u32) -> Icon {
    Icon {
        width: 2,
        height: 2,
        rgba: seed.to_le_bytes().repeat(4),
    }
}

fn with_icon(command: u32, seed: u32) -> MenuItem {
    MenuItem {
        icon: Some(icon(seed)),
        ..shell(command, &format!("Item {command}"), Group::Share)
    }
}

fn cached(cx: &mut TestAppContext) -> usize {
    cx.update(|cx| cx.default_global::<IconCache>().len())
}

#[gpui_kit::test]
fn the_same_icon_in_two_menus_is_uploaded_once(cx: &mut TestAppContext) {
    let menu = || ShellMenu {
        items: vec![with_icon(1, 7), with_icon(2, 7)],
    };
    let first = open_menu_with(cx, point(px(40.), px(20.)), menu());
    let second = open_menu_with(cx, point(px(40.), px(20.)), menu());
    assert_eq!(cached(cx), 1);
    let image = |opened: &super::Opened, cx: &mut TestAppContext| {
        opened
            .menu
            .read_with(cx, |menu, _| menu.icons.image(&[0]).cloned())
            .expect("an icon")
    };
    assert!(Arc::ptr_eq(&image(&first, cx), &image(&second, cx)));
}

#[gpui_kit::test]
fn the_icon_cache_never_holds_more_than_its_limit(cx: &mut TestAppContext) {
    let count = u32::try_from(CACHED_ICONS).unwrap_or(u32::MAX) + 40;
    let menu = ShellMenu {
        items: (0..count).map(|seed| with_icon(seed, seed)).collect(),
    };
    cx.update(|cx| {
        let icons = ShellIcons::build(&menu, cx);
        assert_eq!(icons.len(), usize::try_from(count).unwrap_or(usize::MAX));
    });
    assert_eq!(cached(cx), CACHED_ICONS);
}

#[gpui_kit::test]
fn icons_stay_bounded_when_a_submenu_is_filled_again(cx: &mut TestAppContext) {
    let menu = open_default(cx);
    let targets = || Submenu::Filled((0..3).map(|index| with_icon(60 + index, index)).collect());
    for _ in 0..4 {
        menu.menu.update(cx, |menu, cx| {
            menu.set_submenu(&[SEND_TO], targets(), cx);
        });
    }
    assert_eq!(menu.menu.read_with(cx, |menu, _| menu.icons.len()), 3);
    menu.menu.update(cx, |menu, cx| {
        menu.set_submenu(&[SEND_TO], Submenu::Failed, cx);
    });
    assert_eq!(menu.menu.read_with(cx, |menu, _| menu.icons.len()), 0);
}
