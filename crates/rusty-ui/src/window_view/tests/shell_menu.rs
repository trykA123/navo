use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Entity, InputEvent as _, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Point, Role, TestAppContext, point, px,
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
    live: Cell<usize>,
}

impl Shell {
    fn session(self: &Rc<Self>) -> Option<Result<FakeSession, String>> {
        let answer = self.answer.borrow().clone()?;
        Some(answer.map(|menu| {
            self.live.set(self.live.get() + 1);
            FakeSession {
                menu,
                shell: self.clone(),
            }
        }))
    }
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
        let ready = RefCell::new(shell.session());
        Box::new(move || {
            let answer = ready.borrow_mut().take().or_else(|| shell.session())?;
            Some(answer.map(|session| Box::new(session) as Box<dyn ShellMenuSession>))
        })
    }
}

impl Drop for FakeSession {
    fn drop(&mut self) {
        self.shell.live.set(self.shell.live.get() - 1);
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
    rig_with(cx, true)
}

fn rig_with(cx: &mut TestAppContext, has_shell: bool) -> Rig {
    let fs = fake();
    let state = two_panes(&fs);
    let shell = Rc::new(Shell::default());
    let provider: Rc<dyn ShellMenuProvider> = Rc::new(FakeProvider(shell.clone()));
    let options = StartOptions {
        shell_menu: has_shell.then_some(provider),
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

    fn right_drag_to(&self, cx: &mut TestAppContext, row: usize) {
        let pane = self.pane;
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            let bounds = window
                .within(("list", pane.get()))
                .find(("row", row))
                .bounds();
            let event = MouseMoveEvent {
                position: bounds.center(),
                pressed_button: Some(MouseButton::Right),
                modifiers: Default::default(),
            };
            window.dispatch_event(event.to_platform_input(), cx);
        })
        .expect("window");
        cx.run_until_parked();
        self.frame(cx);
    }

    fn right_up_at(&self, cx: &mut TestAppContext, position: Point<Pixels>) {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            let event = MouseUpEvent {
                button: MouseButton::Right,
                position,
                modifiers: Default::default(),
                click_count: 1,
            };
            window.dispatch_event(event.to_platform_input(), cx);
        })
        .expect("window");
        cx.run_until_parked();
        self.frame(cx);
    }

    fn context_line(&self, cx: &mut TestAppContext) -> Option<String> {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            let root = window.within("context-menu");
            root.try_find(("menu-context", 0usize))
                .and_then(|found| found.label().map(str::to_owned))
        })
        .expect("window")
    }

    fn marked(&self, cx: &mut TestAppContext) -> Vec<u64> {
        self.view.read_with(cx, |view, _| {
            view.browser.state().active_tab().selection().marked()
        })
    }

    fn menu_row_bounds(&self, cx: &mut TestAppContext, row: usize) -> Bounds<Pixels> {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            let mut root = window.within("context-menu");
            let level = root.within(("menu-level", 0usize));
            level.find(("menu-row", row)).bounds()
        })
        .expect("window")
    }

    fn rows(&self, cx: &mut TestAppContext) -> Vec<String> {
        self.level_rows(cx, 0)
    }

    fn level_rows(&self, cx: &mut TestAppContext, depth: usize) -> Vec<String> {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            if window.try_find("context-menu").is_none() {
                return Vec::new();
            }
            let mut root = window.within("context-menu");
            let level = root.within(("menu-level", depth));
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
    rig.wait(cx, 160);
    rig
}

fn reduce_motion(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
}

#[gpui_kit::test]
fn a_long_right_press_opens_the_menu_for_the_row(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.right_button(cx, 1, true);
    assert!(!rig.is_open(cx));
    rig.wait(cx, 560);
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
    rig.wait(cx, 160);
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
fn without_a_windows_menu_only_working_ferrite_actions_show(cx: &mut TestAppContext) {
    let rig = rig_with(cx, false);
    rig.press(cx, "down");
    rig.press(cx, "menu");
    rig.wait(cx, 160);
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
        ]
    );
    let has_footer = cx
        .update_window(rig.window, |_, window, cx| {
            window.render_frame(cx);
            window
                .within("context-menu")
                .try_find("menu-classic")
                .is_some()
        })
        .expect("window");
    assert!(
        !has_footer,
        "the Classic menu button shows without a Windows menu"
    );
    rig.press(cx, "shift-f10");
    assert!(rig.is_open(cx));
    assert_eq!(rig.pane_error(cx), None);
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
fn retry_shows_the_items_of_a_submenu_that_failed_once(cx: &mut TestAppContext) {
    let rig = open_by_key(cx);
    rig.shell
        .fills
        .borrow_mut()
        .insert(vec![2], Submenu::Failed);
    rig.answer(cx, Ok(windows_menu()));
    rig.type_text(cx, "send");
    rig.press(cx, "right");
    assert_eq!(rig.level_rows(cx, 1), ["Could not load", "Retry"]);
    let documents = Submenu::Filled(vec![shell_item(50, "Documents", Group::Share)]);
    rig.shell.fills.borrow_mut().insert(vec![2], documents);
    rig.press(cx, "enter");
    rig.wait(cx, 50);
    assert_eq!(rig.level_rows(cx, 1), ["Documents"]);
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

#[gpui_kit::test]
fn a_right_press_asks_the_shell_before_the_hold_completes(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.right_button(cx, 1, true);
    assert_eq!(
        *rig.shell.opened.borrow(),
        [vec![rig.location("home/a.txt")]]
    );
    assert!(!rig.is_open(cx));
    rig.wait(cx, 560);
    assert!(rig.is_open(cx));
    assert_eq!(rig.shell.opened.borrow().len(), 1);
}

#[gpui_kit::test]
fn a_short_click_drops_the_prefetch_and_its_session(cx: &mut TestAppContext) {
    let rig = rig(cx);
    *rig.shell.answer.borrow_mut() = Some(Ok(windows_menu()));
    rig.right_button(cx, 1, true);
    assert_eq!(rig.shell.live.get(), 1);
    rig.right_button(cx, 1, false);
    rig.wait(cx, 500);
    assert_eq!(rig.shell.live.get(), 0);
    assert!(!rig.is_open(cx));
}

#[gpui_kit::test]
fn a_prefetched_long_press_opens_in_one_piece(cx: &mut TestAppContext) {
    let rig = rig(cx);
    *rig.shell.answer.borrow_mut() = Some(Ok(windows_menu()));
    rig.right_button(cx, 1, true);
    rig.wait(cx, 400);
    let rows = rig.rows(cx);
    assert!(rows.iter().any(|row| row == "7-Zip"), "{rows:?}");
    assert!(!rows.iter().any(|row| row == "Loading Windows items…"));
}

#[gpui_kit::test]
fn items_in_time_open_the_menu_complete_with_no_loading_row(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, "down");
    rig.press(cx, "menu");
    assert!(!rig.is_open(cx));
    rig.wait(cx, 80);
    assert!(!rig.is_open(cx));
    rig.answer(cx, Ok(windows_menu()));
    let rows = rig.rows(cx);
    assert!(rows.iter().any(|row| row == "7-Zip"), "{rows:?}");
    assert!(!rows.iter().any(|row| row == "Loading Windows items…"));
}

#[gpui_kit::test]
fn late_items_merge_without_moving_the_ferrite_rows(cx: &mut TestAppContext) {
    let rig = open_by_key(cx);
    reduce_motion(cx);
    assert!(
        rig.rows(cx)
            .iter()
            .any(|row| row == "Loading Windows items…")
    );
    let before: Vec<_> = (1..=10).map(|row| rig.menu_row_bounds(cx, row)).collect();
    rig.answer(cx, Ok(windows_menu()));
    assert!(rig.rows(cx).iter().any(|row| row == "7-Zip"));
    let after: Vec<_> = (1..=10).map(|row| rig.menu_row_bounds(cx, row)).collect();
    assert_eq!(after, before);
}

#[gpui_kit::test]
fn a_drag_to_another_row_drops_the_prefetch_and_opens_no_menu(cx: &mut TestAppContext) {
    let rig = rig(cx);
    *rig.shell.answer.borrow_mut() = Some(Ok(windows_menu()));
    rig.right_button(cx, 1, true);
    rig.right_drag_to(cx, 0);
    rig.wait(cx, 500);
    assert!(!rig.is_open(cx));
    assert_eq!(rig.shell.live.get(), 0);
}

#[gpui_kit::test]
fn a_long_press_on_an_unmarked_row_adds_it_to_the_marked_rows(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, "insert");
    *rig.shell.answer.borrow_mut() = Some(Ok(windows_menu()));
    rig.right_button(cx, 1, true);
    rig.wait(cx, 450);
    assert!(rig.is_open(cx));
    assert_eq!(rig.marked(cx), [0, 1]);
    let both = vec![rig.location("home/docs"), rig.location("home/a.txt")];
    assert_eq!(*rig.shell.opened.borrow(), [both]);
}

#[gpui_kit::test]
fn an_answer_after_escape_is_dropped(cx: &mut TestAppContext) {
    let rig = open_by_key(cx);
    rig.press(cx, "escape");
    rig.answer(cx, Ok(windows_menu()));
    assert!(!rig.is_open(cx));
    assert_eq!(rig.shell.live.get(), 0);
    let has_run = rig.view.read_with(cx, |view, _| view.menu_run.is_some());
    assert!(!has_run);
}

#[gpui_kit::test]
fn only_the_latest_menu_takes_the_answer(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, "down");
    rig.press(cx, "menu");
    rig.press(cx, "up");
    rig.press(cx, "menu");
    rig.answer(cx, Ok(windows_menu()));
    assert_eq!(rig.shell.opened.borrow().len(), 2);
    assert_eq!(rig.shell.live.get(), 1);
    let context = rig.context_line(cx).unwrap_or_default();
    assert!(context.starts_with("docs"), "{context}");
}

#[gpui_kit::test]
fn properties_asks_the_shell_for_its_properties_verb(cx: &mut TestAppContext) {
    let rig = open_by_key(cx);
    rig.answer(cx, Ok(windows_menu()));
    rig.type_text(cx, "properties");
    rig.press(cx, "enter");
    rig.wait(cx, 50);
    let properties = ItemId::Shell {
        command: 0,
        verb: Some("properties".to_owned()),
    };
    assert_eq!(*rig.shell.invoked.borrow(), [properties]);
}

#[gpui_kit::test]
fn a_right_release_outside_the_list_toggles_once(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.right_button(cx, 1, true);
    rig.right_up_at(cx, point(px(2.), px(2.)));
    rig.wait(cx, 500);
    assert_eq!(rig.marked(cx), [1]);
    assert!(!rig.is_open(cx));
}
