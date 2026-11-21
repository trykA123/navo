//! View tests of the recent-session wiring: compares are recorded, rows follow the store, a row reopens.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use gpui_kit::test::{ClickOptions, TestWindowExt as _};
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, MouseButton, TestAppContext};
use navo_compare_cli::args::SideSpec;
use navo_compare_profile::book::{SessionBook, Visit};
use navo_compare_profile::brand::SLUG;
use navo_compare_profile::session_store::SessionStore;

use sidewise_kit::context_menu::{ContextMenu, ContextMenuEvent};

use crate::compare_tab_script::{install_scripted, rows_script};
use crate::menu_host::menu_view;
use crate::shell::Shell;
use crate::start::{Side, StartEvent};
use crate::start_menu::ForgetSelected;
use crate::start_tabs::StartChoice;
use crate::test_window::open;

static COUNT: AtomicUsize = AtomicUsize::new(0);

/// An exclusive temp folder holding `left/` and `right/`, removed on drop.
struct Fixture(PathBuf);

impl Fixture {
    /// Makes the folder and its two sides.
    fn new() -> Self {
        let name = format!(
            "{SLUG}-h6b-{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::SeqCst)
        );
        let path = std::env::temp_dir().join(name);
        fs::create_dir(&path).expect("create fixture");
        fs::create_dir(path.join("left")).expect("left");
        fs::create_dir(path.join("right")).expect("right");
        Self(path)
    }

    /// The left side.
    fn left(&self) -> PathBuf {
        self.0.join("left")
    }

    /// The right side.
    fn right(&self) -> PathBuf {
        self.0.join("right")
    }

    /// The profile folder the book makes.
    fn profile(&self) -> PathBuf {
        self.0.join("profile")
    }

    /// The sessions the store holds.
    fn stored(&self) -> Vec<navo_compare_profile::session::Session> {
        SessionStore::open(self.profile().join("sessions"))
            .list()
            .expect("list")
    }
}

impl Drop for Fixture {
    /// Removes the folder.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Opens a shell, with the session book over `profile` when given, and renders one frame.
fn open_shell(cx: &mut TestAppContext, profile: Option<&Path>) -> (AnyWindowHandle, Entity<Shell>) {
    install_scripted(cx, rows_script());
    cx.update(|cx| {
        if let Some(dir) = profile {
            Shell::install_sessions(Some(dir), cx).expect("book");
        }
    });
    let opened = open(cx, |window, cx| {
        let shell = Shell::new(window, cx);
        shell.sync_rows(cx);
        shell
    });
    cx.update_window(opened.0, |_, window, cx| window.render_frame(cx))
        .expect("window");
    opened
}

/// Makes the active start screen emit `event`, then renders and settles.
fn emit(
    cx: &mut TestAppContext,
    (window, shell): &(AnyWindowHandle, Entity<Shell>),
    event: StartEvent,
) {
    cx.update_window(*window, |_, window, cx| {
        let screen = shell.read(cx).active_screen().cloned().expect("start tab");
        screen.update(cx, |_, cx| cx.emit(event));
        window.render_frame(cx);
    })
    .expect("window");
    cx.run_until_parked();
}

/// A compare event over the fixture's two sides.
fn compare(fixture: &Fixture) -> StartEvent {
    StartEvent::Compare {
        left: SideSpec::Path(fixture.left()),
        right: SideSpec::Path(fixture.right()),
        choice: StartChoice::default(),
    }
}

#[gpui_kit::test]
fn a_compare_from_start_is_saved_and_listed(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let opened = open_shell(cx, Some(&fixture.profile()));
    emit(cx, &opened, compare(&fixture));
    assert_eq!(fixture.stored().len(), 1);
    let shell = &opened.1;
    let sidebar = shell.read_with(cx, |shell, _| shell.sidebar().clone());
    assert_eq!(
        sidebar.read_with(cx, |sidebar, _| sidebar.rows().recent.len()),
        1
    );
    cx.update_window(opened.0, |_, window, cx| {
        shell.update(cx, |shell, cx| shell.new_tab(window, cx));
    })
    .expect("window");
    let screen = shell.read_with(cx, |shell, _| {
        shell.active_screen().cloned().expect("start")
    });
    assert_eq!(
        screen.read_with(cx, |screen, _| screen.rows.recent.len()),
        1
    );
}

#[gpui_kit::test]
fn an_open_row_reopens_and_bumps_its_session(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let mut book = SessionBook::open(Some(&fixture.profile()), 0).expect("book");
    let visit = Visit::paths(fixture.left(), fixture.right(), false);
    book.record(&visit, 1_000).expect("record");
    let id = book.rows().recent[0].id.clone();
    let opened = open_shell(cx, Some(&fixture.profile()));
    emit(cx, &opened, StartEvent::Open { id });
    let shell = &opened.1;
    assert!(shell.read_with(cx, |shell, _| shell.active_screen().is_none()));
    assert_eq!(shell.read_with(cx, |shell, _| shell.tab_count()), 1);
    let stored = fixture.stored();
    assert_eq!(stored.len(), 1);
    assert!(stored[0].opened > 1);
}

#[gpui_kit::test]
fn a_missing_session_keeps_start_and_shows_one_toast(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let opened = open_shell(cx, Some(&fixture.profile()));
    emit(cx, &opened, StartEvent::Open { id: "abc-0".into() });
    let shell = &opened.1;
    assert!(shell.read_with(cx, |shell, _| shell.active_screen().is_some()));
    assert_eq!(shell.read_with(cx, |shell, cx| shell.toast_count(cx)), 1);
}

#[gpui_kit::test]
fn without_a_book_a_compare_still_opens(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let opened = open_shell(cx, None);
    emit(cx, &opened, compare(&fixture));
    let shell = &opened.1;
    assert!(shell.read_with(cx, |shell, _| shell.active_screen().is_none()));
    assert_eq!(shell.read_with(cx, |shell, cx| shell.toast_count(cx)), 0);
}

#[gpui_kit::test]
fn forget_then_undo_restores_the_row(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let mut book = SessionBook::open(Some(&fixture.profile()), 0).expect("book");
    let visit = Visit::paths(fixture.left(), fixture.right(), false);
    book.record(&visit, 1_000).expect("record");
    let id = book.rows().recent[0].id.clone();
    let opened = open_shell(cx, Some(&fixture.profile()));
    let shell = &opened.1;
    let sidebar = shell.read_with(cx, |shell, _| shell.sidebar().clone());
    let rows =
        |cx: &mut TestAppContext| sidebar.read_with(cx, |sidebar, _| sidebar.rows().recent.len());
    assert_eq!(rows(cx), 1);
    emit(cx, &opened, StartEvent::Forget { id });
    assert_eq!(rows(cx), 0);
    assert!(fixture.stored().is_empty());
    assert_eq!(shell.read_with(cx, |shell, cx| shell.toast_count(cx)), 1);
    let host = shell.read_with(cx, |shell, _| shell.toast_host());
    host.update(cx, |host, cx| host.undo(cx));
    assert_eq!(rows(cx), 1);
    assert_eq!(fixture.stored().len(), 1);
}

/// A fixture whose book holds one recent visit of its two sides.
fn recorded() -> Fixture {
    let fixture = Fixture::new();
    let mut book = SessionBook::open(Some(&fixture.profile()), 0).expect("book");
    let visit = Visit::paths(fixture.left(), fixture.right(), false);
    book.record(&visit, 1_000).expect("record");
    fixture
}

/// Opens a shell over `fixture` (menu keys bound) and right-clicks sidebar Recent entry 0;
/// returns the shell and the open menu.
fn open_recent_menu(
    cx: &mut TestAppContext,
    fixture: &Fixture,
) -> ((AnyWindowHandle, Entity<Shell>), Entity<ContextMenu>) {
    cx.update(|cx| {
        crate::menu_host::bind_menu_keys(cx);
    });
    let (window, shell) = open_shell(cx, Some(&fixture.profile()));
    let menu = right_click_recent(cx, (window, &shell));
    ((window, shell), menu)
}

/// Right-clicks sidebar Recent entry 0 of an open shell; returns the open menu.
fn right_click_recent(
    cx: &mut TestAppContext,
    (window, shell): (AnyWindowHandle, &Entity<Shell>),
) -> Entity<ContextMenu> {
    cx.update_window(window, |_, window, cx| {
        let right = ClickOptions::new().with_button(MouseButton::Right);
        window.click_with_options(("Recent", 0usize), right, cx);
        window.render_frame(cx);
    })
    .expect("right click");
    cx.run_until_parked();
    let menu = shell.read_with(cx, |shell, _| menu_view(&shell.menu));
    menu.expect("open menu")
}

/// MFX: Escape on the sidebar Recent menu gives the focus back to the Start field that had it.
#[gpui_kit::test]
fn escape_on_the_recent_menu_refocuses_the_start_field(cx: &mut TestAppContext) {
    let fixture = recorded();
    cx.update(|cx| {
        crate::menu_host::bind_menu_keys(cx);
    });
    let (window, shell) = open_shell(cx, Some(&fixture.profile()));
    let screen = shell.read_with(cx, |shell, _| shell.active_screen().cloned());
    let screen = screen.expect("start tab");
    cx.update_window(window, |_, window, cx| {
        screen.update(cx, |screen, cx| screen.focus_left(window, cx));
        window.render_frame(cx);
    })
    .expect("window");
    right_click_recent(cx, (window, &shell));
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    let side = cx
        .update_window(window, |_, window, cx| {
            screen.read(cx).focused_side(window, cx)
        })
        .expect("window");
    assert_eq!(
        side,
        Some(Side::Left),
        "Escape did not refocus the Left field"
    );
}

/// Chooses line `index` of `menu`, as a click or Enter would, and settles.
fn choose(cx: &mut TestAppContext, menu: &Entity<ContextMenu>, index: usize) {
    let id = menu.read_with(cx, |menu, _| menu.menu().items[index].id.clone());
    menu.update(cx, |_, cx| cx.emit(ContextMenuEvent::Chosen(id)));
    cx.run_until_parked();
}

/// B5: a right click on a sidebar Recent entry opens the start row menu, and its Remove line
/// forgets the row through the same handler, with the Undo toast.
#[gpui_kit::test]
fn a_sidebar_recent_entry_has_the_row_menu(cx: &mut TestAppContext) {
    let fixture = recorded();
    let ((_, shell), menu) = open_recent_menu(cx, &fixture);
    let items = menu.read_with(cx, |menu, _| menu.menu().items.clone());
    let texts: Vec<_> = items.iter().map(|item| item.text.as_str()).collect();
    assert_eq!(texts, ["Open", "Copy paths", "Remove from recent"]);
    assert!(items.iter().all(|item| item.enabled));
    choose(cx, &menu, 2);
    let sidebar = shell.read_with(cx, |shell, _| shell.sidebar().clone());
    assert_eq!(
        sidebar.read_with(cx, |sidebar, _| sidebar.rows().recent.len()),
        0
    );
    assert!(fixture.stored().is_empty());
    assert_eq!(shell.read_with(cx, |shell, cx| shell.toast_count(cx)), 1);
}

/// B5 follow-up: after Escape closes the Recent menu, a stray Remove at the shell root forgets
/// nothing (the remembered row is cleared).
#[gpui_kit::test]
fn escape_on_the_recent_menu_leaves_no_row_to_remove(cx: &mut TestAppContext) {
    let fixture = recorded();
    let ((window, shell), _menu) = open_recent_menu(cx, &fixture);
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(shell.read_with(cx, |shell, _| menu_view(&shell.menu).is_none()));
    cx.update_window(window, |_, window, cx| {
        let handle = shell.read(cx).handle.clone();
        handle.dispatch_action(&ForgetSelected, window, cx);
    })
    .expect("window");
    cx.run_until_parked();
    assert_eq!(
        fixture.stored().len(),
        1,
        "a dismissed menu forgot a session"
    );
}

/// B5 follow-up: Copy paths on a sidebar Recent entry puts its two sides on the clipboard
/// and confirms with one toast (MFX).
#[gpui_kit::test]
fn copy_paths_on_a_recent_entry_fills_the_clipboard(cx: &mut TestAppContext) {
    let fixture = recorded();
    let ((_, shell), menu) = open_recent_menu(cx, &fixture);
    let sidebar = shell.read_with(cx, |shell, _| shell.sidebar().clone());
    let row = sidebar.read_with(cx, |sidebar, _| sidebar.rows().recent[0].clone());
    choose(cx, &menu, 1);
    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied, Some(format!("{}\n{}", row.left, row.right)));
    let host = shell.read_with(cx, |shell, _| shell.toast_host());
    let shown = host.read_with(cx, |host, _| host.messages());
    assert_eq!(shown, ["Copied paths"]);
}

/// The topbar crumbs' accessible label: title and detail joined by a space.
fn crumbs(cx: &mut TestAppContext, window: AnyWindowHandle) -> String {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .find("top-crumbs")
            .label()
            .unwrap_or_default()
            .to_owned()
    })
    .expect("window")
}

/// B1: the topbar names the active tab, and the start-mode hint shows on start tabs only.
#[gpui_kit::test]
fn the_topbar_follows_the_active_tab_name(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let opened = open_shell(cx, None);
    emit(cx, &opened, compare(&fixture));
    let (window, shell) = (opened.0, &opened.1);
    let title = shell.read_with(cx, |shell, _| shell.tab_title(0));
    assert_ne!(title.as_ref(), "New compare");
    assert_eq!(crumbs(cx, window), title.as_ref());
    cx.update_window(window, |_, window, cx| {
        shell.update(cx, |shell, cx| shell.new_tab(window, cx));
    })
    .expect("window");
    assert_eq!(crumbs(cx, window), "New compare Folder · File");
    cx.update_window(window, |_, window, cx| {
        shell.update(cx, |shell, cx| shell.prev_tab(window, cx));
    })
    .expect("window");
    assert_eq!(crumbs(cx, window), title.as_ref());
}
