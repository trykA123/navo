use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use gpui_kit::{ExternalPaths, FileDropEvent};
use rusty_core::DropKeys;

use super::*;
use crate::window_view::DropPlatform;

pub(super) struct Platform {
    fs: Arc<FakeFs>,
    pub(super) keys: Cell<DropKeys>,
    pub(super) right: Cell<bool>,
    pub(super) allowed: Cell<[bool; 3]>,
    pub(super) reject: Cell<bool>,
    pub(super) offer: Cell<bool>,
    effects: RefCell<Vec<Option<DropOp>>>,
    pub(super) offered: RefCell<Vec<(PathBuf, bool)>>,
}

impl DropPlatform for Platform {
    fn keys(&self, _: Option<isize>) -> Option<DropKeys> {
        Some(self.keys.get())
    }
    fn right_button(&self, _: Option<isize>) -> bool {
        self.right.get()
    }
    fn allows(&self, _: Option<isize>, op: DropOp) -> bool {
        self.allowed.get()[match op {
            DropOp::Copy => 0,
            DropOp::Move => 1,
            DropOp::Link => 2,
        }]
    }
    fn set_effect(&self, _: Option<isize>, op: Option<DropOp>) {
        self.effects.borrow_mut().push(op);
    }
    fn locations(&self, paths: &[PathBuf]) -> Result<Vec<Location>, String> {
        if self.reject.get() {
            return Err("The dropped path is not supported".to_owned());
        }
        Ok(paths
            .iter()
            .map(|path| self.fs.location(&path.to_string_lossy()))
            .collect())
    }
    fn paths(&self, items: &[(Location, bool)]) -> Option<Vec<(PathBuf, bool)>> {
        self.offer.get().then(|| {
            let paths: Vec<_> = items
                .iter()
                .map(|(item, folder)| (PathBuf::from(item.as_str()), *folder))
                .collect();
            self.offered.replace(paths.clone());
            paths
        })
    }
}

pub(super) fn rig_platform(cx: &mut TestAppContext) -> (Rig, Rc<Platform>) {
    let rig = rig(cx);
    let platform = Rc::new(Platform {
        fs: rig.fs.clone(),
        keys: Cell::default(),
        right: Cell::new(false),
        allowed: Cell::new([true; 3]),
        reject: Cell::new(false),
        offer: Cell::new(true),
        effects: RefCell::default(),
        offered: RefCell::default(),
    });
    rig.view
        .update(cx, |view, _| view.drag.platform = Some(platform.clone()));
    (rig, platform)
}

pub(super) fn enter(rig: &Rig, cx: &mut TestAppContext, (path, to): (&str, Point<Pixels>)) {
    dispatch(
        cx,
        rig.window,
        FileDropEvent::Entered {
            position: to,
            paths: ExternalPaths([PathBuf::from(path)].into_iter().collect()),
        }
        .to_platform_input(),
    );
}

pub(super) fn submit(rig: &Rig, cx: &mut TestAppContext, to: Point<Pixels>) {
    dispatch(
        cx,
        rig.window,
        FileDropEvent::Submit { position: to }.to_platform_input(),
    );
    settle(cx, &rig.view);
}

#[gpui_kit::test]
fn incoming_files_use_native_modifiers_and_the_job_planner(cx: &mut TestAppContext) {
    let (rig, platform) = rig_platform(cx);
    platform.keys.set(DropKeys {
        ctrl: true,
        ..DropKeys::default()
    });
    let to = rig.list_spot(cx, rig.dest);
    enter(&rig, cx, ("home/a.txt", to));
    dispatch(
        cx,
        rig.window,
        FileDropEvent::Pending { position: to }.to_platform_input(),
    );
    assert_eq!(platform.effects.borrow().last(), Some(&Some(DropOp::Copy)));
    submit(&rig, cx, to);
    assert_eq!(
        rig.jobs(cx),
        [(OpKind::Copy, Some(rig.fs.location("dest")))]
    );
    assert!(rig.exists("home/a.txt"));
    assert!(rig.exists("dest/a.txt"));
    assert_eq!(rig.focused(cx), rig.home);
}

#[gpui_kit::test]
fn incoming_release_rechecks_shift_without_a_pending_event(cx: &mut TestAppContext) {
    let (rig, platform) = rig_platform(cx);
    platform.keys.set(DropKeys {
        ctrl: true,
        ..DropKeys::default()
    });
    let to = rig.list_spot(cx, rig.dest);
    enter(&rig, cx, ("home/a.txt", to));
    platform.keys.set(DropKeys {
        shift: true,
        ..DropKeys::default()
    });
    submit(&rig, cx, to);
    assert_eq!(
        rig.jobs(cx),
        [(OpKind::Move, Some(rig.fs.location("dest")))]
    );
    assert!(!rig.exists("home/a.txt"));
}

#[gpui_kit::test]
fn source_effect_restrictions_prevent_a_move_job(cx: &mut TestAppContext) {
    let (rig, platform) = rig_platform(cx);
    platform.allowed.set([true, false, false]);
    platform.keys.set(DropKeys {
        shift: true,
        ..DropKeys::default()
    });
    let to = rig.list_spot(cx, rig.dest);
    enter(&rig, cx, ("home/a.txt", to));
    assert_eq!(platform.effects.borrow().last(), Some(&None));
    submit(&rig, cx, to);
    assert!(rig.jobs(cx).is_empty());
    assert!(rig.exists("home/a.txt"));
    assert_eq!(rig.notice(cx), Some(tr("drag-refused-effect")));
}

#[gpui_kit::test]
fn incoming_same_folder_and_descendant_drops_are_refused(cx: &mut TestAppContext) {
    let (rig, platform) = rig_platform(cx);
    let to = rig.list_spot(cx, rig.home);
    enter(&rig, cx, ("home/a.txt", to));
    assert_eq!(platform.effects.borrow().last(), Some(&None));
    submit(&rig, cx, to);
    assert_eq!(rig.notice(cx), Some(tr("drag-refused-same-folder")));
    let to = rig.row_centre(cx, DOCS);
    enter(&rig, cx, ("home/docs", to));
    assert_eq!(platform.effects.borrow().last(), Some(&None));
    submit(&rig, cx, to);
    assert!(rig.jobs(cx).is_empty());
    assert_eq!(rig.notice(cx), Some(tr("drag-refused-into-itself")));
}

#[gpui_kit::test]
fn rejected_incoming_paths_report_the_failure_without_jobs(cx: &mut TestAppContext) {
    let (rig, platform) = rig_platform(cx);
    platform.reject.set(true);
    let to = rig.list_spot(cx, rig.dest);
    enter(&rig, cx, ("home/a.txt", to));
    submit(&rig, cx, to);
    assert!(rig.jobs(cx).is_empty());
    assert_eq!(
        rig.notice(cx).as_deref(),
        Some("The dropped path is not supported")
    );
}

#[gpui_kit::test]
fn leaving_the_window_clears_the_incoming_session(cx: &mut TestAppContext) {
    let (rig, _) = rig_platform(cx);
    let to = rig.list_spot(cx, rig.dest);
    enter(&rig, cx, ("home/a.txt", to));
    assert!(rig.has_session(cx));
    dispatch(cx, rig.window, FileDropEvent::Exited.to_platform_input());
    assert!(!rig.has_session(cx));
    assert!(rig.jobs(cx).is_empty());
}

#[gpui_kit::test]
fn right_button_drop_waits_for_the_chosen_operation(cx: &mut TestAppContext) {
    let (rig, platform) = rig_platform(cx);
    platform.right.set(true);
    let to = rig.list_spot(cx, rig.dest);
    enter(&rig, cx, ("home/a.txt", to));
    submit(&rig, cx, to);
    assert!(rig.jobs(cx).is_empty());
    assert!(
        rig.view
            .read_with(cx, |view, _| view.drop_menu_view().is_some())
    );
    assert_eq!(platform.effects.borrow().last(), Some(&None));
    rig.key(cx, "down");
    rig.key(cx, "enter");
    assert_eq!(
        rig.jobs(cx),
        [(OpKind::Move, Some(rig.fs.location("dest")))]
    );
}

#[gpui_kit::test]
fn escape_cancels_the_right_button_menu(cx: &mut TestAppContext) {
    let (rig, platform) = rig_platform(cx);
    platform.right.set(true);
    let to = rig.list_spot(cx, rig.dest);
    enter(&rig, cx, ("home/a.txt", to));
    submit(&rig, cx, to);
    rig.key(cx, "escape");
    assert!(rig.jobs(cx).is_empty());
    assert!(
        rig.view
            .read_with(cx, |view, _| view.drop_menu_view().is_none())
    );
    assert!(rig.list_has_focus(cx, rig.home));
}

#[gpui_kit::test]
fn an_open_prompt_refuses_incoming_drops(cx: &mut TestAppContext) {
    let (rig, platform) = rig_platform(cx);
    rig.key(cx, "f7");
    assert!(rig.view.read_with(cx, |view, _| view.prompt.is_some()));
    let to = rig.list_spot(cx, rig.dest);
    enter(&rig, cx, ("home/a.txt", to));
    assert_eq!(platform.effects.borrow().last(), Some(&None));
    submit(&rig, cx, to);
    assert!(rig.jobs(cx).is_empty());
    assert!(rig.exists("home/a.txt"));
}

#[gpui_kit::test]
fn a_target_changed_by_navigation_refuses_incoming_drop(cx: &mut TestAppContext) {
    let (rig, _) = rig_platform(cx);
    let to = rig.row_centre(cx, DOCS);
    enter(&rig, cx, ("home/a.txt", to));
    rig.fs.add_folder("other/docs");
    rig.view.update(cx, |view, _| {
        let _ = view.browser.command(&Command::PaneNavigate {
            location: rig.fs.location("other"),
        });
    });
    settle(cx, &rig.view);
    submit(&rig, cx, to);
    assert!(rig.jobs(cx).is_empty());
    assert_eq!(rig.notice(cx), Some(tr("drag-refused-list-changed")));
}

#[gpui_kit::test]
fn right_drop_remembers_the_button_and_offers_only_allowed_choices(cx: &mut TestAppContext) {
    let (rig, platform) = rig_platform(cx);
    platform.right.set(true);
    platform.allowed.set([true, false, false]);
    let to = rig.list_spot(cx, rig.dest);
    enter(&rig, cx, ("home/a.txt", to));
    assert_eq!(platform.effects.borrow().last(), Some(&Some(DropOp::Copy)));
    platform.right.set(false);
    submit(&rig, cx, to);
    let items = rig.view.read_with(cx, |view, cx| {
        view.drop_menu_view()
            .expect("drop menu")
            .read(cx)
            .menu()
            .items
            .clone()
    });
    assert_eq!(
        items.iter().map(|item| item.enabled).collect::<Vec<_>>(),
        [true, false, false, true]
    );
    assert!(rig.jobs(cx).is_empty());
    rig.key(cx, "enter");
    assert_eq!(
        rig.jobs(cx),
        [(OpKind::Copy, Some(rig.fs.location("dest")))]
    );
}

#[gpui_kit::test]
fn an_incoming_drop_on_an_inactive_tab_keeps_that_tab_inactive(cx: &mut TestAppContext) {
    let (rig, platform) = rig_platform(cx);
    platform.keys.set(DropKeys {
        ctrl: true,
        ..DropKeys::default()
    });
    rig.second_tab(cx, rig.dest, "home/docs");
    let to = rig.tab_centre(cx, 1);
    enter(&rig, cx, ("home/a.txt", to));
    assert_eq!(platform.effects.borrow().last(), Some(&Some(DropOp::Copy)));
    submit(&rig, cx, to);
    assert_eq!(
        rig.jobs(cx),
        [(OpKind::Copy, Some(rig.fs.location("home/docs")))]
    );
    assert_eq!(rig.active_tab(cx, rig.dest), 0);
}

#[test]
fn default_incoming_operation_copies_when_any_source_is_on_another_volume() {
    let sources = [
        Location::new(r"C:\one\a.txt").expect("source"),
        Location::new(r"D:\two\b.txt").expect("source"),
    ];
    let target = Location::new(r"C:\dest").expect("target");
    assert_eq!(
        crate::window_view::drop::op_for(&sources, &target, DropKeys::default()),
        DropOp::Copy
    );
}

#[gpui_kit::test]
fn incoming_hover_validates_paths_without_reading_source_metadata(cx: &mut TestAppContext) {
    let (rig, platform) = rig_platform(cx);
    platform.keys.set(DropKeys {
        ctrl: true,
        ..DropKeys::default()
    });
    let to = rig.list_spot(cx, rig.dest);
    enter(&rig, cx, ("home/missing.txt", to));
    assert_eq!(platform.effects.borrow().last(), Some(&Some(DropOp::Copy)));
    assert_eq!(rig.carried(cx), Some(1));
    assert!(rig.notice(cx).is_none());
    assert!(rig.jobs(cx).is_empty());
}

#[gpui_kit::test]
fn navigating_a_hovered_tab_refuses_the_incoming_drop(cx: &mut TestAppContext) {
    let (rig, _) = rig_platform(cx);
    rig.second_tab(cx, rig.dest, "home/docs");
    let to = rig.tab_centre(cx, 1);
    enter(&rig, cx, ("home/a.txt", to));
    rig.view.update(cx, |view, _| {
        view.focus_pane(rig.dest);
        let _ = view.browser.command(&Command::PaneActivateTab { tab: 1 });
        let _ = view.browser.command(&Command::PaneNavigate {
            location: rig.fs.location("home"),
        });
        view.focus_pane(rig.home);
    });
    settle(cx, &rig.view);
    submit(&rig, cx, to);
    assert!(rig.jobs(cx).is_empty());
    assert_eq!(rig.notice(cx), Some(tr("drag-refused-list-changed")));
    assert!(!rig.exists("home/docs/a.txt"));
}

#[gpui_kit::test]
fn replacing_a_hovered_tab_with_the_same_location_refuses_the_drop(cx: &mut TestAppContext) {
    let (rig, _) = rig_platform(cx);
    rig.second_tab(cx, rig.dest, "home/docs");
    let to = rig.tab_centre(cx, 1);
    enter(&rig, cx, ("home/a.txt", to));
    rig.view.update(cx, |view, _| {
        view.focus_pane(rig.dest);
        let _ = view
            .browser
            .command(&Command::PaneCloseTab { tab: Some(1) });
        let _ = view.browser.command(&Command::PaneOpenTab {
            location: Some(rig.fs.location("home/docs")),
        });
        view.focus_pane(rig.home);
    });
    settle(cx, &rig.view);
    submit(&rig, cx, to);
    assert!(rig.jobs(cx).is_empty());
    assert_eq!(rig.notice(cx), Some(tr("drag-refused-list-changed")));
}

#[gpui_kit::test]
fn right_button_drop_can_create_shortcuts_through_the_planner(cx: &mut TestAppContext) {
    let (rig, platform) = rig_platform(cx);
    platform.right.set(true);
    let to = rig.list_spot(cx, rig.dest);
    enter(&rig, cx, ("home/a.txt", to));
    submit(&rig, cx, to);
    rig.key(cx, "down");
    rig.key(cx, "down");
    rig.key(cx, "enter");
    assert_eq!(
        rig.jobs(cx),
        [(OpKind::Link, Some(rig.fs.location("dest")))]
    );
    assert!(rig.exists("home/a.txt"));
}
