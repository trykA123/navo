use std::time::Duration;

use gpui_kit::FileDropEvent;

use super::external::{enter, rig_platform};
use super::*;

impl Rig {
    fn wait(&self, cx: &mut TestAppContext, millis: u64) {
        cx.executor().advance_clock(Duration::from_millis(millis));
        cx.run_until_parked();
        cx.update_window(self.window, |_, window, cx| window.render_frame(cx))
            .expect("window");
    }

    fn move_to(&self, cx: &mut TestAppContext, to: Point<Pixels>) {
        dispatch(
            cx,
            self.window,
            moved(Some(MouseButton::Left), to, Modifiers::default()),
        );
    }

    fn shown(&self, cx: &mut TestAppContext, pane: PaneId) -> Location {
        self.view.read_with(cx, |view, _| {
            view.browser
                .state()
                .pane(pane)
                .map(|pane| pane.active().location().clone())
                .expect("pane")
        })
    }

    fn scroll(&self, cx: &mut TestAppContext, pane: PaneId) -> u64 {
        self.view.read_with(cx, |view, _| {
            view.browser
                .state()
                .pane(pane)
                .map_or(0, |pane| pane.active().view().scroll)
        })
    }

    fn menu_open(&self, cx: &mut TestAppContext) -> Option<PaneId> {
        self.view
            .read_with(cx, |view, _| view.drive_menu.as_ref().map(|menu| menu.pane))
    }

    fn list_edges(&self, cx: &mut TestAppContext, pane: PaneId) -> (Point<Pixels>, Point<Pixels>) {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            let bounds = window.find(("list", pane.get())).bounds();
            let centre = point(bounds.center().x, bounds.center().y);
            (centre, point(centre.x, bounds.bottom() - px(8.)))
        })
        .expect("window")
    }

    fn drive_row(&self, cx: &mut TestAppContext) -> Point<Pixels> {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window.find(("drive-row", 0usize)).bounds().center()
        })
        .expect("window")
    }

    fn many_files(&self, cx: &mut TestAppContext) {
        for index in 0..80 {
            self.fs.add_file(&format!("home/f{index:02}.txt"), 1);
        }
        self.refresh(cx);
    }
}

fn reduce_motion(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
}

#[gpui_kit::test]
fn an_800_ms_hover_on_a_folder_row_opens_it(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let to = rig.row_centre(cx, DOCS);
    rig.drag_to(cx, (A, to), Modifiers::default());
    rig.wait(cx, 790);
    assert_eq!(rig.shown(cx, rig.home), rig.fs.location("home"));
    rig.wait(cx, 20);
    settle(cx, &rig.view);
    assert_eq!(rig.shown(cx, rig.home), rig.fs.location("home/docs"));
    assert!(rig.has_drag(cx));
}

#[gpui_kit::test]
fn leaving_the_folder_at_700_ms_does_not_open_it(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let to = rig.row_centre(cx, DOCS);
    rig.drag_to(cx, (A, to), Modifiers::default());
    rig.wait(cx, 700);
    let away = rig.list_spot(cx, rig.dest);
    rig.move_to(cx, away);
    rig.wait(cx, 1000);
    assert_eq!(rig.shown(cx, rig.home), rig.fs.location("home"));
}

#[gpui_kit::test]
fn a_drop_right_after_hover_open_lands_in_the_opened_folder(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.fs.add_folder("home/docs/inner");
    let to = rig.row_centre(cx, DOCS);
    rig.drag_to(cx, (A, to), Modifiers::default());
    rig.wait(cx, 800);
    settle(cx, &rig.view);
    assert_eq!(rig.shown(cx, rig.home), rig.fs.location("home/docs"));
    let hover = rig.hover(cx).expect("hover");
    assert_eq!(hover.target_label.as_ref(), "docs");
    rig.release(cx, to, Modifiers::default());
    assert!(rig.exists("home/docs/a.txt"));
    assert!(!rig.exists("home/docs/inner/a.txt"));
    assert!(!rig.exists("home/a.txt"));
}

#[gpui_kit::test]
fn escape_cancels_a_pending_hover_open(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let to = rig.row_centre(cx, DOCS);
    rig.drag_to(cx, (A, to), Modifiers::default());
    rig.wait(cx, 400);
    rig.key(cx, "escape");
    rig.wait(cx, 1000);
    assert_eq!(rig.shown(cx, rig.home), rig.fs.location("home"));
    assert!(!rig.has_session(cx));
}

#[gpui_kit::test]
fn a_500_ms_hover_on_a_tab_activates_it(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.second_tab(cx, rig.dest, "home/docs");
    let to = rig.tab_centre(cx, 1);
    rig.drag_to(cx, (A, to), ctrl());
    rig.wait(cx, 400);
    assert_eq!(rig.active_tab(cx, rig.dest), 0);
    rig.wait(cx, 100);
    settle(cx, &rig.view);
    assert_eq!(rig.active_tab(cx, rig.dest), 1);
    let spot = rig.list_spot(cx, rig.dest);
    rig.move_to(cx, spot);
    rig.release(cx, spot, ctrl());
    assert!(rig.exists("home/docs/a.txt"));
    assert!(rig.exists("home/a.txt"));
}

#[gpui_kit::test]
fn an_800_ms_hover_on_the_drive_pill_opens_its_menu_and_rows_take_drops(cx: &mut TestAppContext) {
    let rig = rig_on(cx, (with_fkey_bar(Values::default()), true));
    let root = rig.fs.location("dest").root();
    let to = rig.pill_centre(cx);
    rig.drag_to(cx, (A, to), Modifiers::default());
    rig.wait(cx, 700);
    assert_eq!(rig.menu_open(cx), None);
    rig.wait(cx, 100);
    assert_eq!(rig.menu_open(cx), Some(rig.dest));
    let row = rig.drive_row(cx);
    rig.move_to(cx, row);
    assert_eq!(rig.hover(cx).and_then(|hover| hover.op), Some(DropOp::Move));
    rig.release(cx, row, Modifiers::default());
    assert_eq!(rig.jobs(cx), [(OpKind::Move, Some(root))]);
    assert!(rig.exists("a.txt"));
    assert_eq!(rig.menu_open(cx), None);
}

#[gpui_kit::test]
fn escape_closes_a_drive_menu_the_hover_opened(cx: &mut TestAppContext) {
    let rig = rig_on(cx, (with_fkey_bar(Values::default()), true));
    let to = rig.pill_centre(cx);
    rig.drag_to(cx, (A, to), Modifiers::default());
    rig.wait(cx, 800);
    assert_eq!(rig.menu_open(cx), Some(rig.dest));
    rig.key(cx, "escape");
    assert_eq!(rig.menu_open(cx), None);
    assert!(rig.list_has_focus(cx, rig.home));
}

#[gpui_kit::test]
fn reduce_motion_turns_hover_open_off(cx: &mut TestAppContext) {
    reduce_motion(cx);
    let rig = rig_on(cx, (with_fkey_bar(Values::default()), true));
    rig.second_tab(cx, rig.dest, "home/docs");
    let to = rig.row_centre(cx, DOCS);
    rig.drag_to(cx, (A, to), Modifiers::default());
    rig.wait(cx, 2000);
    assert_eq!(rig.shown(cx, rig.home), rig.fs.location("home"));
    let tab = rig.tab_centre(cx, 1);
    rig.move_to(cx, tab);
    rig.wait(cx, 2000);
    assert_eq!(rig.active_tab(cx, rig.dest), 0);
    let pill = rig.pill_centre(cx);
    rig.move_to(cx, pill);
    rig.wait(cx, 2000);
    assert_eq!(rig.menu_open(cx), None);
    assert!(rig.has_drag(cx));
}

#[gpui_kit::test]
fn reduce_motion_turns_auto_scroll_off(cx: &mut TestAppContext) {
    reduce_motion(cx);
    let rig = rig(cx);
    rig.many_files(cx);
    let (_, bottom) = rig.list_edges(cx, rig.home);
    rig.drag_to(cx, (A, bottom), Modifiers::default());
    rig.wait(cx, 1000);
    assert_eq!(rig.scroll(cx, rig.home), 0);
}

#[gpui_kit::test]
fn auto_scroll_advances_one_row_per_50_ms_only_inside_the_band(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.many_files(cx);
    let (centre, bottom) = rig.list_edges(cx, rig.home);
    rig.drag_to(cx, (A, centre), Modifiers::default());
    rig.wait(cx, 500);
    assert_eq!(rig.scroll(cx, rig.home), 0);
    rig.move_to(cx, bottom);
    for _ in 0..4 {
        rig.wait(cx, 50);
    }
    assert_eq!(rig.scroll(cx, rig.home), 4);
    rig.move_to(cx, centre);
    rig.wait(cx, 500);
    assert_eq!(rig.scroll(cx, rig.home), 4);
    rig.move_to(cx, bottom);
    rig.wait(cx, 50);
    rig.key(cx, "escape");
    rig.wait(cx, 500);
    assert_eq!(rig.scroll(cx, rig.home), 5);
}

#[gpui_kit::test]
fn auto_scroll_runs_up_in_the_top_band_and_stops_on_drop(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.many_files(cx);
    let (_, bottom) = rig.list_edges(cx, rig.home);
    rig.drag_to(cx, (A, bottom), Modifiers::default());
    for _ in 0..6 {
        rig.wait(cx, 50);
    }
    assert_eq!(rig.scroll(cx, rig.home), 6);
    let top = cx
        .update_window(rig.window, |_, window, cx| {
            window.render_frame(cx);
            let bounds = window
                .within(("list", rig.home.get()))
                .find("list")
                .bounds();
            point(bounds.center().x, bounds.top() + px(8.))
        })
        .expect("window");
    rig.move_to(cx, top);
    for _ in 0..2 {
        rig.wait(cx, 50);
    }
    assert_eq!(rig.scroll(cx, rig.home), 4);
    rig.release(cx, top, Modifiers::default());
    rig.wait(cx, 500);
    assert_eq!(rig.scroll(cx, rig.home), 4);
    assert!(!rig.has_session(cx));
}

#[gpui_kit::test]
fn a_release_off_any_target_after_a_tab_spring_focuses_the_source_pane(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.second_tab(cx, rig.dest, "home/docs");
    let to = rig.tab_centre(cx, 1);
    rig.drag_to(cx, (A, to), Modifiers::default());
    rig.wait(cx, 600);
    assert_eq!(rig.focused(cx), rig.dest);
    let off = point(px(2.), px(2.));
    rig.move_to(cx, off);
    rig.release(cx, off, Modifiers::default());
    rig.wait(cx, 100);
    assert_eq!(rig.focused(cx), rig.home);
    assert!(rig.list_has_focus(cx, rig.home));
}

#[gpui_kit::test]
fn a_drop_after_scrolling_out_of_a_sprung_folder_targets_the_row_under_the_pointer(
    cx: &mut TestAppContext,
) {
    let rig = rig(cx);
    for index in 0..90 {
        rig.fs.add_folder(&format!("home/docs/sub{index:02}"));
    }
    rig.refresh(cx);
    let to = rig.row_centre(cx, DOCS);
    rig.drag_to(cx, (A, to), Modifiers::default());
    rig.wait(cx, 800);
    settle(cx, &rig.view);
    assert_eq!(rig.shown(cx, rig.home), rig.fs.location("home/docs"));
    let (_, bottom) = rig.list_edges(cx, rig.home);
    rig.move_to(cx, bottom);
    for _ in 0..2 {
        rig.wait(cx, 50);
    }
    assert!(rig.scroll(cx, rig.home) > 0);
    rig.release(cx, bottom, Modifiers::default());
    assert!(!rig.exists("home/docs/a.txt"));
    let inside = (0..90).any(|index| rig.exists(&format!("home/docs/sub{index:02}/a.txt")));
    assert!(inside);
}

#[gpui_kit::test]
fn leaving_the_window_cancels_a_pending_hover_open_of_an_external_drag(cx: &mut TestAppContext) {
    let (rig, _) = rig_platform(cx);
    let to = rig.row_centre(cx, DOCS);
    enter(&rig, cx, ("dest/x.txt", to));
    rig.wait(cx, 400);
    dispatch(cx, rig.window, FileDropEvent::Exited.to_platform_input());
    assert!(!rig.has_session(cx));
    rig.wait(cx, 1000);
    assert_eq!(rig.shown(cx, rig.home), rig.fs.location("home"));
}
