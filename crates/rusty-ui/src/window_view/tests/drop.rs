use std::sync::Arc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Capslock, Entity, InputEvent as _, Modifiers,
    ModifiersChangedEvent, MouseButton, Pixels, Point, TestAppContext, point, px,
};
use rusty_core::{Command, DropOp, Location, OpKind, PaneId};
use rusty_fs::{FakeFs, Provider};
use rusty_l10n::tr;

use super::{home_beside_dest, open_options, options_for, press, settle, with_fkey_bar};
use crate::drag::Hover;
use crate::pane_chrome::path_segments;
use crate::settings::{Change, Field, Value, Values};
use crate::test_pointer::{self, ctrl, dispatch, moved, up};
use crate::window_view::{DriveIcon, DriveInfo, FerriteWindow};

const DOCS: usize = 0;
const A: usize = 1;
const B: usize = 2;
const C: usize = 3;

struct Rig {
    window: AnyWindowHandle,
    view: Entity<FerriteWindow>,
    fs: Arc<FakeFs>,
    home: PaneId,
    dest: PaneId,
}

fn files() -> Arc<FakeFs> {
    let fs = FakeFs::new();
    fs.add_folder("home/docs");
    fs.add_file("home/docs/report.txt", 7);
    for name in ["a.txt", "b.txt", "c.txt"] {
        fs.add_file(&format!("home/{name}"), 3);
    }
    fs.add_folder("dest");
    Arc::new(fs)
}

fn rig_with(cx: &mut TestAppContext, values: Values) -> Rig {
    rig_on(cx, (values, false))
}

fn rig_on(cx: &mut TestAppContext, (values, with_drive): (Values, bool)) -> Rig {
    let fs = files();
    let mut options = options_for(fs.clone(), home_beside_dest(&fs));
    options.settings.values = values;
    if with_drive {
        let drive = DriveInfo {
            root: fs.location("dest").root(),
            letter: None,
            icon: DriveIcon::Fixed,
            label: Some("Test".to_owned()),
            remote: None,
            connected: true,
            space: None,
        };
        options.drives.refresh = Arc::new(move || vec![drive.clone()]);
    }
    let (window, view) = open_options(cx, options);
    let (home, dest) = view.read_with(cx, |view, _| {
        let state = view.browser.state();
        let pane_at = |path: &str| {
            view.panes()
                .into_iter()
                .find(|pane| {
                    state.pane(*pane).map(|p| p.active().location()) == Some(&fs.location(path))
                })
                .expect("pane")
        };
        (pane_at("home"), pane_at("dest"))
    });
    Rig {
        window,
        view,
        fs,
        home,
        dest,
    }
}

fn rig(cx: &mut TestAppContext) -> Rig {
    rig_with(cx, with_fkey_bar(Values::default()))
}

fn alt() -> Modifiers {
    Modifiers {
        alt: true,
        ..Modifiers::default()
    }
}

impl Rig {
    fn grip(&self, cx: &mut TestAppContext, row: usize) -> Point<Pixels> {
        let pane = self.home.get();
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            let bounds = window
                .within(("list", pane))
                .within(("row", row))
                .find("grip")
                .bounds();
            point(bounds.right() - px(4.), bounds.center().y)
        })
        .expect("window")
    }

    fn list_spot(&self, cx: &mut TestAppContext, pane: PaneId) -> Point<Pixels> {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            let bounds = window.find(("list", pane.get())).bounds();
            point(bounds.center().x, bounds.bottom() - px(24.))
        })
        .expect("window")
    }

    fn row_centre(&self, cx: &mut TestAppContext, row: usize) -> Point<Pixels> {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window
                .within(("list", self.home.get()))
                .find(("row", row))
                .bounds()
                .center()
        })
        .expect("window")
    }

    fn drag_to(&self, cx: &mut TestAppContext, (row, to): (usize, Point<Pixels>), mods: Modifiers) {
        let from = self.grip(cx, row);
        let nudge = point(from.x + px(12.), from.y);
        test_pointer::drag(cx, self.window, (MouseButton::Left, mods), &[from, nudge]);
        dispatch(cx, self.window, moved(Some(MouseButton::Left), to, mods));
    }

    fn release(&self, cx: &mut TestAppContext, at: Point<Pixels>, mods: Modifiers) {
        dispatch(cx, self.window, up(MouseButton::Left, at, mods));
        settle(cx, &self.view);
    }

    fn drop_on(&self, cx: &mut TestAppContext, (row, to): (usize, Point<Pixels>), mods: Modifiers) {
        self.drag_to(cx, (row, to), mods);
        self.release(cx, to, mods);
    }

    fn second_tab(&self, cx: &mut TestAppContext, pane: PaneId, path: &str) {
        let open = Command::PaneOpenTab {
            location: Some(self.fs.location(path)),
        };
        let back = Command::PaneActivateTab { tab: 0 };
        cx.update_window(self.window, |_, window, cx| {
            self.view.update(cx, |view, cx| {
                view.focus_pane(pane);
                view.apply(&open, window, cx);
                view.apply(&back, window, cx);
                view.focus_pane(self.home);
            });
        })
        .expect("window");
        settle(cx, &self.view);
    }

    fn tab_centre(&self, cx: &mut TestAppContext, tab: usize) -> Point<Pixels> {
        let pane = self.dest.get();
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window.within(("tabs", pane)).find(tab).bounds().center()
        })
        .expect("window")
    }

    fn crumb_centre(&self, cx: &mut TestAppContext, crumb: usize) -> Point<Pixels> {
        let pane = self.dest.get();
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window
                .within(("path", pane))
                .find(("segment", crumb))
                .bounds()
                .center()
        })
        .expect("window")
    }

    fn pill_centre(&self, cx: &mut TestAppContext) -> Point<Pixels> {
        let pane = self.dest.get();
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window.find(("drive-pill", pane)).bounds().center()
        })
        .expect("window")
    }

    fn active_tab(&self, cx: &mut TestAppContext, pane: PaneId) -> usize {
        self.view.read_with(cx, |view, _| {
            view.browser
                .state()
                .pane(pane)
                .map_or(0, |pane| pane.active_index())
        })
    }

    fn key(&self, cx: &mut TestAppContext, key: &str) {
        press(cx, self.window, &self.view, key);
    }

    fn mark(&self, cx: &mut TestAppContext, rows: &[usize]) {
        self.key(cx, "home");
        let mut at = 0;
        for row in rows {
            for _ in at..*row {
                self.key(cx, "down");
            }
            at = *row;
            self.key(cx, "space");
        }
    }

    fn marks(&self, cx: &mut TestAppContext) -> Vec<u64> {
        self.view.read_with(cx, |view, _| {
            view.browser
                .state()
                .pane(self.home)
                .map(|pane| pane.active().selection().marked())
                .unwrap_or_default()
        })
    }

    fn hover(&self, cx: &mut TestAppContext) -> Option<Hover> {
        self.view.read_with(cx, |view, cx| {
            view.drag_session
                .as_ref()
                .and_then(|session| session.state.read(cx).hover.clone())
        })
    }

    fn carried(&self, cx: &mut TestAppContext) -> Option<usize> {
        self.view.read_with(cx, |view, cx| {
            view.drag_session
                .as_ref()
                .map(|session| session.state.read(cx).count)
        })
    }

    fn jobs(&self, cx: &mut TestAppContext) -> Vec<(OpKind, Option<Location>)> {
        self.view.read_with(cx, |view, _| {
            view.browser
                .jobs()
                .iter()
                .map(|job| (job.kind, job.target.clone()))
                .collect()
        })
    }

    fn notice(&self, cx: &mut TestAppContext) -> Option<String> {
        self.view.read_with(cx, |view, _| {
            view.status_notice().map(|notice| notice.text.to_string())
        })
    }

    fn exists(&self, path: &str) -> bool {
        self.fs.metadata(&self.fs.location(path)).is_ok()
    }

    fn focused(&self, cx: &mut TestAppContext) -> PaneId {
        self.view
            .read_with(cx, |view, _| view.browser.state().focused_pane())
    }

    fn list_has_focus(&self, cx: &mut TestAppContext, pane: PaneId) -> bool {
        cx.update_window(self.window, |_, window, cx| {
            self.view
                .read(cx)
                .focus
                .get(&pane)
                .is_some_and(|focus| focus.is_focused(window))
        })
        .expect("window")
    }

    fn has_drag(&self, cx: &mut TestAppContext) -> bool {
        cx.update(|cx| cx.has_active_drag())
    }
}

#[gpui_kit::test]
fn a_file_dropped_on_the_other_pane_moves_there(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let to = rig.list_spot(cx, rig.dest);
    rig.drop_on(cx, (A, to), Modifiers::default());
    assert_eq!(
        rig.jobs(cx),
        [(OpKind::Move, Some(rig.fs.location("dest")))]
    );
    assert!(rig.exists("dest/a.txt"));
    assert!(!rig.exists("home/a.txt"));
    assert!(!rig.has_drag(cx));
}

#[gpui_kit::test]
fn ctrl_held_on_release_copies(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let to = rig.list_spot(cx, rig.dest);
    rig.drop_on(cx, (A, to), ctrl());
    assert_eq!(
        rig.jobs(cx),
        [(OpKind::Copy, Some(rig.fs.location("dest")))]
    );
    assert!(rig.exists("dest/a.txt"));
    assert!(rig.exists("home/a.txt"));
}

#[gpui_kit::test]
fn dragging_a_marked_row_carries_every_marked_row(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.mark(cx, &[A, B, C]);
    let to = rig.list_spot(cx, rig.dest);
    rig.drag_to(cx, (B, to), Modifiers::default());
    assert_eq!(rig.carried(cx), Some(3));
    rig.release(cx, to, Modifiers::default());
    for name in ["a.txt", "b.txt", "c.txt"] {
        assert!(rig.exists(&format!("dest/{name}")), "{name}");
    }
    assert!(rig.exists("home/docs"));
}

#[gpui_kit::test]
fn dragging_an_unmarked_row_carries_only_it_and_keeps_the_marks(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.mark(cx, &[A, C]);
    let to = rig.list_spot(cx, rig.dest);
    rig.drag_to(cx, (B, to), ctrl());
    assert_eq!(rig.carried(cx), Some(1));
    assert_eq!(rig.marks(cx), [1, 3]);
    rig.release(cx, to, ctrl());
    assert!(rig.exists("dest/b.txt"));
    assert!(!rig.exists("dest/a.txt"));
    assert!(!rig.exists("dest/c.txt"));
}

#[gpui_kit::test]
fn a_drop_on_a_folder_row_goes_into_that_folder(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let to = rig.row_centre(cx, DOCS);
    rig.drag_to(cx, (A, to), Modifiers::default());
    let hover = rig.hover(cx).expect("hover");
    assert_eq!(hover.op, Some(DropOp::Move));
    assert_eq!(hover.target_label.as_ref(), "docs");
    rig.release(cx, to, Modifiers::default());
    assert!(rig.exists("home/docs/a.txt"));
    assert!(!rig.exists("home/a.txt"));
}

#[gpui_kit::test]
fn a_drop_on_its_own_folder_is_refused_with_a_notice(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let to = rig.list_spot(cx, rig.home);
    rig.drag_to(cx, (A, to), Modifiers::default());
    let hover = rig.hover(cx).expect("hover");
    assert!(hover.refusal.is_some());
    assert_eq!(hover.op, None);
    rig.release(cx, to, Modifiers::default());
    assert!(rig.jobs(cx).is_empty());
    assert_eq!(rig.notice(cx), Some(tr("drag-refused-same-folder")));
    assert!(rig.exists("home/a.txt"));
}

#[gpui_kit::test]
fn a_folder_dropped_on_itself_is_refused(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let to = rig.row_centre(cx, DOCS);
    let from = rig.grip(cx, DOCS);
    let nudge = point(from.x + px(12.), from.y);
    let mods = Modifiers::default();
    test_pointer::drag(
        cx,
        rig.window,
        (MouseButton::Left, mods),
        &[from, nudge, to],
    );
    rig.release(cx, to, mods);
    assert!(rig.jobs(cx).is_empty());
    assert_eq!(rig.notice(cx), Some(tr("drag-refused-into-itself")));
}

#[gpui_kit::test]
fn escape_cancels_the_drag_without_a_job_and_keeps_the_marks(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.mark(cx, &[A]);
    let to = rig.list_spot(cx, rig.dest);
    rig.drag_to(cx, (A, to), Modifiers::default());
    assert!(rig.has_drag(cx));
    rig.key(cx, "escape");
    assert!(!rig.has_drag(cx));
    rig.release(cx, to, Modifiers::default());
    assert!(rig.jobs(cx).is_empty());
    assert!(rig.exists("home/a.txt"));
    assert_eq!(rig.marks(cx), [1]);
    assert_eq!(rig.focused(cx), rig.home);
}

#[gpui_kit::test]
fn drop_confirm_moves_asks_before_a_dragged_move(cx: &mut TestAppContext) {
    let values = with_fkey_bar(Values::default()).with(&Change {
        field: Field::DropConfirm,
        value: Value::Pick(1),
    });
    let rig = rig_with(cx, values);
    let to = rig.list_spot(cx, rig.dest);
    rig.drop_on(cx, (A, to), Modifiers::default());
    let pending = rig.view.read_with(cx, |view, _| {
        view.browser.pending().map(|pending| pending.plan.kind)
    });
    assert_eq!(pending, Some(OpKind::Move));
    assert!(rig.exists("home/a.txt"));
    rig.key(cx, "escape");
    rig.drop_on(cx, (B, to), ctrl());
    assert!(rig.exists("dest/b.txt"));
    assert!(rig.exists("home/a.txt"));
}

#[gpui_kit::test]
fn alt_shows_shortcuts_in_the_badge_and_a_drop_runs_nothing(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let to = rig.list_spot(cx, rig.dest);
    rig.drag_to(cx, (A, to), alt());
    assert_eq!(rig.hover(cx).and_then(|hover| hover.op), Some(DropOp::Link));
    rig.release(cx, to, alt());
    assert!(rig.jobs(cx).is_empty());
    assert!(rig.exists("home/a.txt"));
    assert!(!rig.exists("dest/a.txt"));
    assert_eq!(rig.notice(cx), Some(tr("drag-link-later")));
}

#[gpui_kit::test]
fn a_modifier_change_during_the_drag_updates_the_badge(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let to = rig.list_spot(cx, rig.dest);
    rig.drag_to(cx, (A, to), Modifiers::default());
    assert_eq!(rig.hover(cx).and_then(|hover| hover.op), Some(DropOp::Move));
    let changed = ModifiersChangedEvent {
        modifiers: ctrl(),
        capslock: Capslock::default(),
    };
    dispatch(cx, rig.window, changed.to_platform_input());
    assert_eq!(rig.hover(cx).and_then(|hover| hover.op), Some(DropOp::Copy));
    rig.release(cx, to, ctrl());
    assert!(rig.exists("home/a.txt"));
}

#[gpui_kit::test]
fn focus_stays_in_the_source_pane_after_a_drop(cx: &mut TestAppContext) {
    let rig = rig(cx);
    assert_eq!(rig.focused(cx), rig.home);
    let to = rig.list_spot(cx, rig.dest);
    rig.drop_on(cx, (A, to), Modifiers::default());
    assert_eq!(rig.focused(cx), rig.home);
    assert!(rig.list_has_focus(cx, rig.home));
    let cursor = |cx: &mut TestAppContext| {
        rig.view.read_with(cx, |view, _| {
            view.browser
                .state()
                .pane(rig.home)
                .map(|pane| pane.active().view().cursor)
        })
    };
    let before = cursor(cx).expect("cursor");
    rig.key(cx, "down");
    assert_eq!(cursor(cx), Some(before + 1));
}

#[gpui_kit::test]
fn a_drop_on_another_tab_copies_into_its_folder_without_activating_it(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.second_tab(cx, rig.dest, "home/docs");
    let to = rig.tab_centre(cx, 1);
    rig.drag_to(cx, (A, to), ctrl());
    let hover = rig.hover(cx).expect("hover");
    assert_eq!(hover.op, Some(DropOp::Copy));
    assert_eq!(hover.target_label.as_ref(), "docs");
    rig.release(cx, to, ctrl());
    assert_eq!(
        rig.jobs(cx),
        [(OpKind::Copy, Some(rig.fs.location("home/docs")))]
    );
    assert!(rig.exists("home/docs/a.txt"));
    assert!(rig.exists("home/a.txt"));
    assert_eq!(rig.active_tab(cx, rig.dest), 0);
}

#[gpui_kit::test]
fn a_drop_on_the_tab_of_the_sources_own_folder_is_refused(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.second_tab(cx, rig.dest, "home");
    let to = rig.tab_centre(cx, 1);
    rig.drop_on(cx, (A, to), Modifiers::default());
    assert!(rig.jobs(cx).is_empty());
    assert_eq!(rig.notice(cx), Some(tr("drag-refused-same-folder")));
}

#[gpui_kit::test]
fn a_drop_on_the_root_crumb_targets_the_root(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let root = path_segments(&rig.fs.location("dest"))[0].location.clone();
    let to = rig.crumb_centre(cx, 0);
    rig.drag_to(cx, (A, to), Modifiers::default());
    assert_eq!(
        rig.hover(cx).map(|hover| hover.op),
        Some(Some(DropOp::Move))
    );
    rig.release(cx, to, Modifiers::default());
    assert_eq!(rig.jobs(cx), [(OpKind::Move, Some(root))]);
}

#[gpui_kit::test]
fn a_drop_on_the_drive_pill_targets_the_drive_root(cx: &mut TestAppContext) {
    let rig = rig_on(cx, (with_fkey_bar(Values::default()), true));
    let root = rig.fs.location("dest").root();
    let to = rig.pill_centre(cx);
    rig.drag_to(cx, (A, to), Modifiers::default());
    let hover = rig.hover(cx).expect("hover");
    assert_eq!(hover.op, Some(DropOp::Move));
    rig.release(cx, to, Modifiers::default());
    assert_eq!(rig.jobs(cx), [(OpKind::Move, Some(root))]);
    assert!(rig.exists("a.txt"));
    assert!(!rig.exists("home/a.txt"));
}

#[gpui_kit::test]
fn a_drop_on_the_pill_of_the_sources_own_root_is_refused(cx: &mut TestAppContext) {
    let rig = rig_on(cx, (with_fkey_bar(Values::default()), true));
    rig.fs.add_file("top.txt", 2);
    let root = Command::PaneNavigate {
        location: rig.fs.location("dest").root(),
    };
    cx.update_window(rig.window, |_, window, cx| {
        rig.view
            .update(cx, |view, cx| view.apply(&root, window, cx));
    })
    .expect("window");
    settle(cx, &rig.view);
    let to = rig.pill_centre(cx);
    rig.drag_to(cx, (2, to), Modifiers::default());
    assert!(rig.hover(cx).expect("hover").refusal.is_some());
    rig.release(cx, to, Modifiers::default());
    assert!(rig.jobs(cx).is_empty());
    assert_eq!(rig.notice(cx), Some(tr("drag-refused-same-folder")));
    assert!(rig.exists("top.txt"));
}
