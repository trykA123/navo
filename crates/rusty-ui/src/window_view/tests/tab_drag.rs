use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, Modifiers, MouseButton, Pixels, Point,
    TestAppContext, VisualTestContext, point, px, size,
};
use rusty_core::{Command, Location, PaneId, apply};

use super::{fake, open_options_sized, options_for, press, settle, two_panes};
use crate::test_pointer::{click, ctrl, dispatch, down, moved, shift, up};
use crate::window_view::FerriteWindow;

struct Rig {
    window: AnyWindowHandle,
    view: Entity<FerriteWindow>,
    left: PaneId,
    right: PaneId,
}

fn open_tab(state: &mut rusty_core::AppState, location: Location) {
    apply(
        state,
        &Command::PaneOpenTab {
            location: Some(location),
        },
    )
    .expect("tab");
}

fn rig(cx: &mut TestAppContext) -> Rig {
    let fs = fake();
    let mut state = two_panes(&fs);
    state.focus_number(2).expect("second pane");
    apply(
        &mut state,
        &Command::PaneNavigate {
            location: fs.location("dest"),
        },
    )
    .expect("dest");
    open_tab(&mut state, fs.location("home"));
    state.focus_number(1).expect("first pane");
    open_tab(&mut state, fs.location("home/docs"));
    open_tab(&mut state, fs.location("dest"));
    apply(&mut state, &Command::PaneActivateTab { tab: 0 }).expect("first tab");
    let (window, view) = open_options_sized(cx, options_for(fs, state), px(1280.));
    VisualTestContext::from_window(window, cx).simulate_resize(size(px(1280.), px(800.)));
    cx.run_until_parked();
    let panes = view.read_with(cx, |view, _| view.core_layout().unwrap().numbered());
    Rig {
        window,
        view,
        left: panes[0],
        right: panes[1],
    }
}

impl Rig {
    fn folders(&self, cx: &TestAppContext, pane: PaneId) -> Vec<String> {
        self.view.read_with(cx, |view, _| {
            let pane = view.browser.state().pane(pane).unwrap();
            pane.tabs()
                .iter()
                .map(|tab| {
                    tab.location()
                        .as_str()
                        .rsplit('/')
                        .next()
                        .unwrap()
                        .to_owned()
                })
                .collect()
        })
    }

    fn active(&self, cx: &TestAppContext) -> (PaneId, usize) {
        self.view.read_with(cx, |view, _| {
            let state = view.browser.state();
            let pane = state.focused_pane();
            (pane, state.pane(pane).unwrap().active_index())
        })
    }

    fn tab(&self, cx: &mut TestAppContext, pane: PaneId, index: usize) -> gpui_kit::Bounds<Pixels> {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window.within(("tabs", pane.get())).find(index).bounds()
        })
        .expect("window")
    }

    fn body(&self, cx: &mut TestAppContext, pane: PaneId) -> Point<Pixels> {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window.find(("list", pane.get())).bounds().center()
        })
        .expect("window")
    }

    fn start(
        &self,
        cx: &mut TestAppContext,
        (from, to): (Point<Pixels>, Point<Pixels>),
        keys: Modifiers,
    ) {
        dispatch(
            cx,
            self.window,
            down(MouseButton::Left, from, Modifiers::default()),
        );
        dispatch(cx, self.window, moved(Some(MouseButton::Left), to, keys));
    }

    fn release(&self, cx: &mut TestAppContext, to: Point<Pixels>, keys: Modifiers) {
        dispatch(cx, self.window, up(MouseButton::Left, to, keys));
        settle(cx, &self.view);
    }

    fn shows(&self, cx: &mut TestAppContext, id: &'static str) -> bool {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window.try_find(id).is_some()
        })
        .expect("window")
    }
}

fn between(a: gpui_kit::Bounds<Pixels>, b: gpui_kit::Bounds<Pixels>) -> Point<Pixels> {
    point((a.right() + b.left()) / 2., a.center().y)
}

#[gpui_kit::test]
fn drag_to_another_row_inserts_at_the_hovered_gap(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let from = rig.tab(cx, rig.left, 0).center();
    let to = between(rig.tab(cx, rig.right, 0), rig.tab(cx, rig.right, 1));
    rig.start(cx, (from, to), Modifiers::default());
    assert!(rig.shows(cx, "tab-drag-chip"));
    assert!(rig.shows(cx, "tab-insert-line"));
    rig.release(cx, to, Modifiers::default());
    assert_eq!(rig.folders(cx, rig.left), ["docs", "dest"]);
    assert_eq!(rig.folders(cx, rig.right), ["dest", "home", "home"]);
    assert_eq!(rig.active(cx), (rig.right, 1));
    assert!(!rig.shows(cx, "tab-drag-chip"));
    assert!(!rig.shows(cx, "tab-insert-line"));
}

#[gpui_kit::test]
fn ctrl_at_drop_copies_and_keeps_the_source(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let from = rig.tab(cx, rig.left, 1).center();
    let first = rig.tab(cx, rig.right, 0);
    let to = point(first.left() + px(1.), first.center().y);
    rig.start(cx, (from, to), ctrl());
    rig.release(cx, to, ctrl());
    assert_eq!(rig.folders(cx, rig.left), ["home", "docs", "dest"]);
    assert_eq!(rig.folders(cx, rig.right), ["docs", "dest", "home"]);
    assert_eq!(rig.active(cx), (rig.right, 0));
}

#[gpui_kit::test]
fn drop_on_another_pane_body_appends_and_outlines_it(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let from = rig.tab(cx, rig.left, 2).center();
    let to = rig.body(cx, rig.right);
    rig.start(cx, (from, to), Modifiers::default());
    cx.update_window(rig.window, |_, window, _| {
        assert!(
            window
                .find(("pane-swap-outline", rig.right.get()))
                .visible()
        );
    })
    .expect("window");
    assert!(!rig.shows(cx, "tab-insert-line"));
    rig.release(cx, to, Modifiers::default());
    assert_eq!(rig.folders(cx, rig.left), ["home", "docs"]);
    assert_eq!(rig.folders(cx, rig.right), ["dest", "home", "dest"]);
    assert_eq!(rig.active(cx), (rig.right, 2));
}

#[gpui_kit::test]
fn drag_within_a_row_reorders(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let from = rig.tab(cx, rig.left, 0).center();
    let last = rig.tab(cx, rig.left, 2);
    let to = point(last.right() - px(2.), last.center().y);
    rig.start(cx, (from, to), Modifiers::default());
    rig.release(cx, to, Modifiers::default());
    assert_eq!(rig.folders(cx, rig.left), ["docs", "dest", "home"]);
    assert_eq!(rig.active(cx), (rig.left, 2));
}

#[gpui_kit::test]
fn escape_outside_and_short_drags_change_nothing(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let before = (rig.folders(cx, rig.left), rig.folders(cx, rig.right));
    let from = rig.tab(cx, rig.left, 0).center();
    let to = rig.body(cx, rig.right);
    rig.start(cx, (from, to), Modifiers::default());
    press(cx, rig.window, &rig.view, "escape");
    assert!(!rig.shows(cx, "tab-drag-chip"));
    rig.release(cx, to, Modifiers::default());
    let outside = point(px(-20.), px(-20.));
    rig.start(cx, (from, outside), Modifiers::default());
    assert!(rig.shows(cx, "tab-drag-chip"));
    rig.release(cx, outside, Modifiers::default());
    let short = point(from.x + px(5.), from.y);
    rig.start(cx, (from, short), Modifiers::default());
    assert!(!rig.shows(cx, "tab-drag-chip"));
    rig.release(cx, short, Modifiers::default());
    assert_eq!(
        (rig.folders(cx, rig.left), rig.folders(cx, rig.right)),
        before
    );
    assert!(
        !rig.view
            .read_with(cx, |view, _| view.tab_drag.is_dragging())
    );
}

#[gpui_kit::test]
fn click_on_a_tab_still_activates_it(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let at = rig.tab(cx, rig.right, 1).center();
    click(cx, rig.window, (at, Modifiers::default()));
    settle(cx, &rig.view);
    assert_eq!(rig.active(cx), (rig.right, 1));
    assert_eq!(rig.folders(cx, rig.right), ["dest", "home"]);
}

#[gpui_kit::test]
fn click_on_the_empty_header_bar_activates_that_pane(cx: &mut TestAppContext) {
    let rig = rig(cx);
    assert_eq!(rig.active(cx), (rig.left, 0));
    let last = rig.tab(cx, rig.right, 1);
    let card = cx
        .update_window(rig.window, |_, window, cx| {
            window.render_frame(cx);
            window.find(("pane", rig.right.get())).bounds()
        })
        .expect("window");
    let at = point((last.right() + card.right()) / 2., last.center().y);
    click(cx, rig.window, (at, Modifiers::default()));
    settle(cx, &rig.view);
    assert_eq!(rig.active(cx).0, rig.right);
    assert_eq!(rig.folders(cx, rig.right), ["dest", "home"]);
}

#[gpui_kit::test]
fn shift_drag_on_a_tab_still_swaps_panes(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let left = rig.folders(cx, rig.left);
    let right = rig.folders(cx, rig.right);
    let from = rig.tab(cx, rig.left, 0).center();
    let to = rig.body(cx, rig.right);
    dispatch(cx, rig.window, down(MouseButton::Left, from, shift()));
    dispatch(cx, rig.window, moved(Some(MouseButton::Left), to, shift()));
    assert!(!rig.shows(cx, "tab-drag-chip"));
    assert!(rig.shows(cx, "pane-swap-chip"));
    rig.release(cx, to, shift());
    let numbered = rig
        .view
        .read_with(cx, |view, _| view.core_layout().unwrap().numbered());
    assert_eq!(numbered, [rig.right, rig.left]);
    assert_eq!(rig.folders(cx, rig.left), left);
    assert_eq!(rig.folders(cx, rig.right), right);
}

#[gpui_kit::test]
fn file_drag_from_a_row_starts_no_tab_drag(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let before = rig.folders(cx, rig.left);
    let from = cx
        .update_window(rig.window, |_, window, cx| {
            window.render_frame(cx);
            window
                .within(("list", rig.left.get()))
                .within(("row", 1usize))
                .find("grip")
                .bounds()
                .center()
        })
        .expect("window");
    let to = point(from.x + px(12.), from.y);
    rig.start(cx, (from, to), Modifiers::default());
    assert!(!rig.shows(cx, "tab-drag-chip"));
    assert!(
        rig.view
            .read_with(cx, |view, _| view.drag.session.is_some())
    );
    press(cx, rig.window, &rig.view, "escape");
    assert_eq!(rig.folders(cx, rig.left), before);
}
