use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, InputEvent as _, MouseButton, MouseDownEvent,
    MouseUpEvent, Role, TestAppContext,
};
use rusty_core::shell_menu::{Group, ItemId, ItemKind, MenuItem, ShellMenu, Submenu};
use rusty_core::{Location, PaneId};
use rusty_fs::FakeFs;

use super::super::shell_menu::{Reply, ScreenSpot, ShellMenuProvider, ShellMenuSession};
use super::super::{FerriteWindow, StartOptions};
use super::{fake, open_options, settle, two_panes};

#[derive(Default)]
struct Shell {
    answer: RefCell<Option<Result<ShellMenu, String>>>,
    opened: RefCell<Vec<Vec<Location>>>,
    fills: RefCell<HashMap<Vec<usize>, Submenu>>,
    asked: RefCell<Vec<Vec<usize>>>,
    invoked: RefCell<Vec<ItemId>>,
    invoke_error: RefCell<Option<String>>,
    classic: Cell<usize>,
}

struct FakeProvider(Rc<Shell>);

struct FakeSession {
    menu: ShellMenu,
    shell: Rc<Shell>,
}

impl ShellMenuProvider for FakeProvider {
    fn open(&self, items: &[Location]) -> Reply<Box<dyn ShellMenuSession>> {
        self.0.opened.borrow_mut().push(items.to_vec());
        let shell = self.0.clone();
        Box::new(move || {
            let answer = shell.answer.borrow().clone()?;
            Some(answer.map(|menu| {
                let session = FakeSession {
                    menu,
                    shell: shell.clone(),
                };
                Box::new(session) as Box<dyn ShellMenuSession>
            }))
        })
    }
}

impl ShellMenuSession for FakeSession {
    fn menu(&self) -> &ShellMenu {
        &self.menu
    }

    fn fill(&self, path: &[usize]) -> Reply<Submenu> {
        self.shell.asked.borrow_mut().push(path.to_vec());
        let shell = self.shell.clone();
        let path = path.to_vec();
        Box::new(move || shell.fills.borrow().get(&path).cloned().map(Ok))
    }

    fn invoke(&self, id: &ItemId, _: ScreenSpot) -> Reply<()> {
        self.shell.invoked.borrow_mut().push(id.clone());
        let error = self.shell.invoke_error.borrow().clone();
        Box::new(move || Some(error.clone().map_or(Ok(()), Err)))
    }

    fn classic(&self, _: ScreenSpot) -> Reply<()> {
        self.shell.classic.set(self.shell.classic.get() + 1);
        Box::new(|| Some(Ok(())))
    }
}

fn shell_item(command: u32, raw: &str, group: Group) -> MenuItem {
    MenuItem::new(
        ItemId::Shell {
            command,
            verb: None,
        },
        raw,
        group,
    )
}

fn windows_menu() -> ShellMenu {
    let seven_zip = MenuItem {
        kind: ItemKind::Submenu(Submenu::Filled(vec![shell_item(
            30,
            "Extract Here",
            Group::Extensions,
        )])),
        ..shell_item(3, "7-Zip", Group::Extensions)
    };
    let send_to = MenuItem {
        kind: ItemKind::Submenu(Submenu::Lazy),
        ..shell_item(5, "&Send to", Group::Share)
    };
    ShellMenu {
        items: vec![
            shell_item(1, "&Open", Group::Open),
            seven_zip,
            send_to,
            shell_item(7, "Cu&t\tCtrl+X", Group::Edit),
        ],
    }
}

struct Rig {
    fs: Arc<FakeFs>,
    window: AnyWindowHandle,
    view: Entity<FerriteWindow>,
    shell: Rc<Shell>,
    pane: PaneId,
}

fn rig(cx: &mut TestAppContext) -> Rig {
    let fs = fake();
    let state = two_panes(&fs);
    let shell = Rc::new(Shell::default());
    let options = StartOptions {
        shell_menu: Some(Rc::new(FakeProvider(shell.clone()))),
        ..super::options_for(fs.clone(), state)
    };
    let (window, view) = open_options(cx, options);
    let pane = view.read_with(cx, |view, _| view.browser.state().focused_pane());
    Rig {
        fs,
        window,
        view,
        shell,
        pane,
    }
}

impl Rig {
    fn frame(&self, cx: &mut TestAppContext) {
        cx.update_window(self.window, |_, window, cx| window.render_frame(cx))
            .expect("window");
        cx.run_until_parked();
    }

    fn press(&self, cx: &mut TestAppContext, key: &str) {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window.press(key, cx);
        })
        .expect("window");
        cx.run_until_parked();
        self.frame(cx);
    }

    fn type_text(&self, cx: &mut TestAppContext, text: &str) {
        cx.update_window(self.window, |_, window, cx| window.input(text, cx))
            .expect("window");
        cx.run_until_parked();
        self.frame(cx);
    }

    fn wait(&self, cx: &mut TestAppContext, millis: u64) {
        cx.executor().advance_clock(Duration::from_millis(millis));
        cx.run_until_parked();
        self.frame(cx);
    }

    fn right_button(&self, cx: &mut TestAppContext, row: usize, down: bool) {
        let pane = self.pane;
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            let bounds = window
                .within(("list", pane.get()))
                .find(("row", row))
                .bounds();
            let position = bounds.center();
            let event = if down {
                MouseDownEvent {
                    button: MouseButton::Right,
                    position,
                    modifiers: Default::default(),
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input()
            } else {
                MouseUpEvent {
                    button: MouseButton::Right,
                    position,
                    modifiers: Default::default(),
                    click_count: 1,
                }
                .to_platform_input()
            };
            window.dispatch_event(event, cx);
        })
        .expect("window");
        cx.run_until_parked();
        self.frame(cx);
    }

    fn rows(&self, cx: &mut TestAppContext) -> Vec<String> {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            if window.try_find("context-menu").is_none() {
                return Vec::new();
            }
            let mut root = window.within("context-menu");
            let level = root.within(("menu-level", 0usize));
            (0usize..)
                .map_while(|index| level.try_find(("menu-row", index)))
                .map(|row| match (row.role(), row.label()) {
                    (Some(Role::Heading), Some(label)) => format!("# {label}"),
                    (_, Some(label)) => label.to_owned(),
                    (_, None) => "-".to_owned(),
                })
                .collect()
        })
        .expect("window")
    }

    fn is_open(&self, cx: &mut TestAppContext) -> bool {
        !self.rows(cx).is_empty()
    }

    fn list_has_focus(&self, cx: &mut TestAppContext) -> bool {
        let pane = self.pane;
        let handle = self.view.update(cx, |view, cx| view.handle(pane, cx));
        cx.update_window(self.window, |_, window, _| handle.is_focused(window))
            .expect("window")
    }

    fn pane_error(&self, cx: &mut TestAppContext) -> Option<String> {
        let pane = self.pane;
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window
                .try_find(("pane-error", pane.get()))
                .and_then(|found| found.label().map(str::to_owned))
        })
        .expect("window")
    }

    fn answer(&self, cx: &mut TestAppContext, menu: Result<ShellMenu, String>) {
        *self.shell.answer.borrow_mut() = Some(menu);
        self.wait(cx, 50);
    }

    fn location(&self, name: &str) -> Location {
        self.fs.location(name)
    }
}

fn open_by_key(cx: &mut TestAppContext) -> Rig {
    let rig = rig(cx);
    rig.press(cx, "down");
    rig.press(cx, "menu");
    rig
}

#[gpui_kit::test]
fn a_long_right_press_opens_the_menu_for_the_row(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.right_button(cx, 1, true);
    assert!(!rig.is_open(cx));
    rig.wait(cx, 450);
    assert!(rig.is_open(cx));
    assert_eq!(
        *rig.shell.opened.borrow(),
        [vec![rig.location("home/a.txt")]]
    );
    rig.right_button(cx, 1, false);
    assert!(rig.is_open(cx));
    let marked = rig.view.read_with(cx, |view, _| {
        view.browser.state().active_tab().selection().count()
    });
    assert_eq!(marked, 0);
}

#[gpui_kit::test]
fn a_short_right_click_selects_the_row_without_a_menu(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.right_button(cx, 1, true);
    rig.wait(cx, 100);
    rig.right_button(cx, 1, false);
    rig.wait(cx, 500);
    assert!(!rig.is_open(cx));
    let marked = rig.view.read_with(cx, |view, _| {
        view.browser.state().active_tab().selection().marked()
    });
    assert_eq!(marked, [1]);
}

#[gpui_kit::test]
fn the_menu_key_opens_the_menu_for_the_cursor_row(cx: &mut TestAppContext) {
    let rig = open_by_key(cx);
    assert!(rig.is_open(cx));
    assert_eq!(
        *rig.shell.opened.borrow(),
        [vec![rig.location("home/a.txt")]]
    );
}

#[gpui_kit::test]
fn shift_f10_opens_the_menu_for_the_marked_rows(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, "insert");
    rig.press(cx, "insert");
    rig.press(cx, "shift-f10");
    assert!(rig.is_open(cx));
    let expected = vec![rig.location("home/docs"), rig.location("home/a.txt")];
    assert_eq!(*rig.shell.opened.borrow(), [expected]);
}

#[gpui_kit::test]
fn ferrite_actions_show_before_the_provider_answers(cx: &mut TestAppContext) {
    let rig = open_by_key(cx);
    assert_eq!(
        rig.rows(cx),
        [
            "# Ferrite",
            "Open",
            "View",
            "Edit",
            "Copy",
            "Move",
            "Rename",
            "Delete",
            "Copy path",
            "Properties",
            "Classic menu",
            "# Windows",
            "Loading Windows items…",
        ]
    );
}

#[gpui_kit::test]
fn provider_items_merge_in_when_they_arrive(cx: &mut TestAppContext) {
    let rig = open_by_key(cx);
    rig.press(cx, "down");
    rig.answer(cx, Ok(windows_menu()));
    let rows = rig.rows(cx);
    assert!(rows.iter().any(|row| row == "7-Zip"));
    assert!(rows.iter().any(|row| row == "Cut"));
    assert!(!rows.iter().any(|row| row == "Loading Windows items…"));
    let selected = cx
        .update_window(rig.window, |_, window, _| {
            let mut root = window.within("context-menu");
            let level = root.within(("menu-level", 0usize));
            (0usize..)
                .map_while(|index| level.try_find(("menu-row", index)))
                .find(|row| row.selected() == Some(true))
                .and_then(|row| row.label().map(str::to_owned))
        })
        .expect("window");
    assert_eq!(selected.as_deref(), Some("View"));
}

#[gpui_kit::test]
fn a_loading_submenu_fills_when_the_shell_answers(cx: &mut TestAppContext) {
    let rig = open_by_key(cx);
    rig.answer(cx, Ok(windows_menu()));
    let send_to = vec![2];
    assert_eq!(*rig.shell.asked.borrow(), [vec![2]]);
    rig.type_text(cx, "docu");
    assert_eq!(rig.rows(cx), ["No matching commands"]);
    let documents = Submenu::Filled(vec![shell_item(50, "Documents", Group::Share)]);
    rig.shell.fills.borrow_mut().insert(send_to, documents);
    rig.wait(cx, 50);
    assert_eq!(rig.rows(cx), ["Send to › Documents"]);
}

#[gpui_kit::test]
fn invoking_a_windows_item_calls_the_provider(cx: &mut TestAppContext) {
    let rig = open_by_key(cx);
    rig.answer(cx, Ok(windows_menu()));
    rig.type_text(cx, "cut");
    rig.press(cx, "enter");
    rig.wait(cx, 50);
    let cut = ItemId::Shell {
        command: 7,
        verb: None,
    };
    assert_eq!(*rig.shell.invoked.borrow(), [cut]);
    assert!(!rig.is_open(cx));
}

#[gpui_kit::test]
fn the_classic_menu_asks_the_provider(cx: &mut TestAppContext) {
    let rig = open_by_key(cx);
    rig.answer(cx, Ok(windows_menu()));
    rig.press(cx, "shift-f10");
    rig.wait(cx, 50);
    assert_eq!(rig.shell.classic.get(), 1);
    assert!(!rig.is_open(cx));
}

#[gpui_kit::test]
fn a_classic_menu_asked_early_waits_for_the_shell(cx: &mut TestAppContext) {
    let rig = open_by_key(cx);
    rig.press(cx, "shift-f10");
    assert_eq!(rig.shell.classic.get(), 0);
    rig.answer(cx, Ok(windows_menu()));
    assert_eq!(rig.shell.classic.get(), 1);
}

#[gpui_kit::test]
fn focus_returns_to_the_list_when_the_menu_closes(cx: &mut TestAppContext) {
    let rig = open_by_key(cx);
    assert!(!rig.list_has_focus(cx));
    rig.press(cx, "escape");
    assert!(!rig.is_open(cx));
    assert!(rig.list_has_focus(cx));
}

#[gpui_kit::test]
fn a_ferrite_action_runs_the_existing_command(cx: &mut TestAppContext) {
    let rig = open_by_key(cx);
    rig.type_text(cx, "rename");
    rig.press(cx, "enter");
    settle(cx, &rig.view);
    assert!(!rig.is_open(cx));
    let renaming = rig.view.read_with(cx, |view, _| view.prompt.is_some());
    assert!(renaming);
}

#[gpui_kit::test]
fn copy_path_puts_the_paths_on_the_clipboard(cx: &mut TestAppContext) {
    let rig = open_by_key(cx);
    rig.type_text(cx, "copy path");
    rig.press(cx, "enter");
    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied, Some(rig.location("home/a.txt").as_str().to_owned()));
}

#[gpui_kit::test]
fn a_provider_error_lands_in_the_status_line(cx: &mut TestAppContext) {
    let rig = open_by_key(cx);
    *rig.shell.invoke_error.borrow_mut() = Some("Access is denied.".to_owned());
    rig.answer(cx, Ok(windows_menu()));
    rig.type_text(cx, "cut");
    rig.press(cx, "enter");
    rig.wait(cx, 50);
    assert_eq!(
        rig.pane_error(cx).as_deref(),
        Some("Cut: Access is denied.")
    );
}

#[gpui_kit::test]
fn a_menu_the_shell_cannot_read_keeps_the_ferrite_actions(cx: &mut TestAppContext) {
    let rig = open_by_key(cx);
    rig.answer(cx, Err("The device is not ready.".to_owned()));
    let rows = rig.rows(cx);
    assert_eq!(rows.last().map(String::as_str), Some("Classic menu"));
    assert_eq!(
        rig.pane_error(cx).as_deref(),
        Some("Windows menu: The device is not ready.")
    );
}
