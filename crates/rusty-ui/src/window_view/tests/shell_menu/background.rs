use std::rc::Rc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext as _, InputEvent as _, MouseButton, MouseDownEvent, Pixels, Point, TestAppContext,
    point, px,
};
use rusty_core::AppState;
use rusty_core::shell_menu::{Group, ItemId, ItemKind, MenuItem, ShellMenu, Submenu};

use super::super::clipboard::FakeClipboard;
use super::super::{fake, options_for, settle, two_panes, type_in};
use super::{Rig, rig, rig_from, rig_on, rig_with, shell_item};
use crate::file_list::SortBy;
use crate::window_view::{ClipboardEffect, ClipboardFiles, FileClipboardService};

const LOADING_ROWS: [&str; 7] = [
    "Sort by",
    "Refresh",
    "Copy folder path",
    "Open terminal here",
    "-",
    "Loading Windows items…",
    "New file…",
];

fn folder_menu() -> ShellMenu {
    let new = MenuItem {
        kind: ItemKind::Submenu(Submenu::Filled(vec![
            shell_item(60, "&Folder", Group::Other),
            shell_item(61, "Text Document", Group::Other),
        ])),
        ..shell_item(6, "Ne&w", Group::Other)
    };
    let properties = MenuItem::new(
        ItemId::Shell {
            command: 12,
            verb: Some("properties".to_owned()),
        },
        "&Properties",
        Group::Other,
    );
    ShellMenu {
        items: vec![
            shell_item(9, "Open in Terminal", Group::Open),
            MenuItem::separator(Group::Open),
            new,
            MenuItem::separator(Group::Other),
            properties,
        ],
    }
}

fn rig_in_empty_folder(cx: &mut TestAppContext) -> Rig {
    let fs = fake();
    let state = AppState::new(fs.location("dest"));
    rig_on(cx, true, (fs, state))
}

impl Rig {
    fn empty_spot(&self, cx: &mut TestAppContext) -> Point<Pixels> {
        let pane = self.pane;
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            let list = window.find(("list", pane.get())).bounds();
            point(list.center().x, list.bottom() - px(12.))
        })
        .expect("window")
    }

    fn right_down_at(&self, cx: &mut TestAppContext, position: Point<Pixels>) {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            let event = MouseDownEvent {
                button: MouseButton::Right,
                position,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            };
            window.dispatch_event(event.to_platform_input(), cx);
        })
        .expect("window");
        cx.run_until_parked();
        self.frame(cx);
    }

    fn open_background(&self, cx: &mut TestAppContext) {
        let spot = self.empty_spot(cx);
        self.right_down_at(cx, spot);
        self.wait(cx, 160);
    }

    fn cursor(&self, cx: &mut TestAppContext) -> u64 {
        self.view.read_with(cx, |view, _| {
            view.browser.state().active_tab().view().cursor
        })
    }

    fn menu_item(&self, cx: &mut TestAppContext, text: &str) -> Option<MenuItem> {
        self.view.update(cx, |view, cx| {
            let menu = view.shell_menu_view()?.read(cx).menu().clone();
            find(&menu.items, text)
        })
    }

    fn choose(&self, cx: &mut TestAppContext, filter: &str) {
        self.type_text(cx, filter);
        self.press(cx, "enter");
        settle(cx, &self.view);
    }

    fn prompt_open(&self, cx: &mut TestAppContext) -> bool {
        self.view.read_with(cx, |view, _| view.prompt.is_some())
    }

    fn cursor_name(&self, cx: &mut TestAppContext) -> Option<String> {
        self.view.read_with(cx, |view, _| {
            let tab = view.browser.state().active_tab();
            tab.listing()
                .and_then(|listing| listing.get(tab.view().cursor))
                .map(|entry| entry.name.clone())
        })
    }

    fn exists(&self, path: &str) -> bool {
        use rusty_fs::Provider as _;
        self.fs.metadata(&self.fs.location(path)).is_ok()
    }
}

fn find(items: &[MenuItem], text: &str) -> Option<MenuItem> {
    items.iter().find_map(|item| {
        if item.text == text {
            return Some(item.clone());
        }
        item.children().and_then(|children| find(children, text))
    })
}

#[gpui_kit::test]
fn a_right_click_below_the_rows_opens_the_folder_menu(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.open_background(cx);
    assert_eq!(rig.rows(cx), LOADING_ROWS);
    assert_eq!(*rig.shell.backgrounds.borrow(), [rig.location("home")]);
    assert!(rig.shell.opened.borrow().is_empty());
}

#[gpui_kit::test]
fn the_folder_menu_names_the_folder_and_its_item_count(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.open_background(cx);
    assert_eq!(rig.context_line(cx).as_deref(), Some("home · 2 items"));
}

#[gpui_kit::test]
fn a_right_click_on_the_empty_area_keeps_the_cursor_and_marks(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, "insert");
    assert_eq!((rig.marked(cx), rig.cursor(cx)), (vec![0], 1));
    rig.open_background(cx);
    assert!(rig.is_open(cx));
    assert_eq!((rig.marked(cx), rig.cursor(cx)), (vec![0], 1));
    rig.press(cx, "escape");
    rig.wait(cx, 500);
    assert_eq!((rig.marked(cx), rig.cursor(cx)), (vec![0], 1));
}

#[gpui_kit::test]
fn a_right_click_in_an_empty_folder_opens_the_folder_menu(cx: &mut TestAppContext) {
    let rig = rig_in_empty_folder(cx);
    rig.open_background(cx);
    assert!(rig.is_open(cx));
    assert_eq!(*rig.shell.backgrounds.borrow(), [rig.location("dest")]);
    assert_eq!(rig.context_line(cx).as_deref(), Some("dest · 0 items"));
}

#[gpui_kit::test]
fn the_menu_key_in_an_empty_folder_opens_the_folder_menu(cx: &mut TestAppContext) {
    let rig = rig_in_empty_folder(cx);
    rig.press(cx, "menu");
    rig.wait(cx, 160);
    assert_eq!(rig.rows(cx), LOADING_ROWS);
    assert_eq!(*rig.shell.backgrounds.borrow(), [rig.location("dest")]);
}

#[gpui_kit::test]
fn a_long_right_press_on_a_row_still_opens_the_item_menu(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.right_button(cx, 1, true);
    rig.wait(cx, 560);
    assert!(rig.is_open(cx));
    assert_eq!(
        *rig.shell.opened.borrow(),
        [vec![rig.location("home/a.txt")]]
    );
    assert!(rig.shell.backgrounds.borrow().is_empty());
}

#[gpui_kit::test]
fn sort_by_and_refresh_lead_and_new_file_follows_new(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.open_background(cx);
    rig.answer(cx, Ok(folder_menu()));
    assert_eq!(
        rig.rows(cx),
        [
            "Sort by",
            "Refresh",
            "Copy folder path",
            "Open terminal here",
            "-",
            "Open in Terminal",
            "-",
            "New",
            "New file…",
            "-",
            "Properties",
        ]
    );
}

#[gpui_kit::test]
fn without_a_windows_menu_the_folder_menu_lists_every_ferrite_item(cx: &mut TestAppContext) {
    let rig = rig_with(cx, false, &[]);
    rig.open_background(cx);
    assert_eq!(
        rig.rows(cx),
        [
            "New folder",
            "New file…",
            "Paste",
            "Refresh",
            "Sort by",
            "Copy folder path",
            "Open terminal here",
        ]
    );
}

#[gpui_kit::test]
fn a_windows_new_item_is_invoked_on_the_folder_menu(cx: &mut TestAppContext) {
    let rig = rig(cx);
    *rig.shell.answer.borrow_mut() = Some(Ok(folder_menu()));
    rig.open_background(cx);
    rig.choose(cx, "text document");
    rig.wait(cx, 50);
    let text_document = ItemId::Shell {
        command: 61,
        verb: None,
    };
    assert_eq!(*rig.shell.invoked.borrow(), [text_document]);
}

#[gpui_kit::test]
fn paste_looks_disabled_with_nothing_to_paste(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.open_background(cx);
    let paste = rig.menu_item(cx, "Paste").expect("a Paste item");
    assert!(!paste.enabled);
}

#[gpui_kit::test]
fn a_folder_outside_the_windows_shell_has_no_properties(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.open_background(cx);
    assert!(rig.menu_item(cx, "Properties").is_none());
}

#[gpui_kit::test]
fn sort_by_checks_the_current_order_and_changes_it(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.open_background(cx);
    let checked = |rig: &Rig, cx: &mut TestAppContext, text| {
        rig.menu_item(cx, text).is_some_and(|item| item.checked)
    };
    assert!(checked(&rig, cx, "Name ↑"));
    assert!(!checked(&rig, cx, "Date"));
    rig.type_text(cx, "date");
    assert_eq!(rig.rows(cx), ["Sort by › Date"]);
    rig.press(cx, "enter");
    settle(cx, &rig.view);
    let sort = rig.view.read_with(cx, |view, _| {
        let tab = view.browser.state().active_tab().id();
        view.browser.sort_key(tab).by
    });
    assert_eq!(sort, SortBy::Date);
}

#[gpui_kit::test]
fn choosing_the_checked_order_reverses_it_and_the_menu_says_so(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.open_background(cx);
    rig.choose(cx, "name");
    let descending = rig.view.read_with(cx, |view, _| {
        let tab = view.browser.state().active_tab().id();
        view.browser.sort_key(tab).descending
    });
    assert!(descending);
    rig.open_background(cx);
    let name = rig
        .menu_item(cx, "Name ↓")
        .expect("the reversed order on Name");
    assert!(name.checked);
}

#[gpui_kit::test]
fn paste_is_enabled_when_the_clipboard_holds_files(cx: &mut TestAppContext) {
    let fs = fake();
    let clipboard = FakeClipboard::default();
    *clipboard.held.borrow_mut() = Some(ClipboardFiles {
        items: vec![fs.location("dest")],
        effect: ClipboardEffect::Copy,
    });
    let shared: Rc<dyn FileClipboardService> = Rc::new(clipboard);
    let options = crate::window_view::StartOptions {
        file_clipboard: Some(shared),
        ..options_for(fs.clone(), two_panes(&fs))
    };
    let rig = rig_from(cx, true, (fs, options));
    rig.open_background(cx);
    let paste = rig.menu_item(cx, "Paste").expect("a Paste item");
    assert!(paste.enabled);
}

#[gpui_kit::test]
fn copy_folder_path_puts_the_folder_on_the_clipboard(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.open_background(cx);
    rig.choose(cx, "copy folder path");
    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied, Some(rig.location("home").as_str().to_owned()));
}

#[gpui_kit::test]
fn new_folder_from_the_folder_menu_asks_for_a_name(cx: &mut TestAppContext) {
    let rig = rig_in_empty_folder(cx);
    rig.open_background(cx);
    rig.choose(cx, "new folder");
    assert!(rig.prompt_open(cx));
    rig.press(cx, "enter");
    settle(cx, &rig.view);
    assert!(rig.exists("dest/New folder"));
}

#[gpui_kit::test]
fn new_file_from_the_folder_menu_creates_and_selects_the_file(cx: &mut TestAppContext) {
    let rig = rig_in_empty_folder(cx);
    rig.open_background(cx);
    rig.choose(cx, "new file");
    assert!(rig.prompt_open(cx));
    rig.press(cx, "enter");
    settle(cx, &rig.view);
    rig.frame(cx);
    assert!(!rig.prompt_open(cx));
    assert!(rig.exists("dest/New file.txt"));
    assert_eq!(rig.cursor_name(cx).as_deref(), Some("New file.txt"));
}

#[gpui_kit::test]
fn shift_f7_names_the_new_file_with_the_stem_selected(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, "shift-f7");
    assert!(rig.prompt_open(cx));
    type_in(cx, rig.window, &rig.view, "notes");
    rig.press(cx, "enter");
    settle(cx, &rig.view);
    rig.frame(cx);
    assert!(rig.exists("home/notes.txt"));
    assert_eq!(rig.cursor_name(cx).as_deref(), Some("notes.txt"));
}

#[gpui_kit::test]
fn escape_cancels_the_new_file_name(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, "shift-f7");
    rig.press(cx, "escape");
    settle(cx, &rig.view);
    assert!(!rig.prompt_open(cx));
    assert!(!rig.exists("home/New file.txt"));
    assert!(rig.list_has_focus(cx));
}

#[gpui_kit::test]
fn a_new_file_name_that_is_already_here_is_refused(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, "shift-f7");
    type_in(cx, rig.window, &rig.view, "a");
    rig.press(cx, "enter");
    assert!(rig.prompt_open(cx));
    let error = cx
        .update_window(rig.window, |_, window, cx| {
            window.render_frame(cx);
            window.try_find("error").map(|found| found.visible())
        })
        .expect("window");
    assert_eq!(error, Some(true));
    let size = {
        use rusty_fs::Provider as _;
        rig.fs
            .metadata(&rig.fs.location("home/a.txt"))
            .map(|e| e.size)
    };
    assert_eq!(size.ok(), Some(3));
}
