use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Capslock, Entity, InputEvent as _, Modifiers,
    ModifiersChangedEvent, MouseButton, Pixels, Point, TestAppContext, VisualTestContext, point,
    px, size,
};
use rusty_core::{Command, Location, PaneId, Tiles, apply};

use super::{fake, open_options_sized, options_for, press, settle, two_panes};
use crate::test_pointer::{click, dispatch, down, moved, shift, up};
use crate::tokens::{self, Mode};
use crate::window_view::FerriteWindow;

struct Rig {
    window: AnyWindowHandle,
    view: Entity<FerriteWindow>,
    panes: Vec<PaneId>,
}

fn rig(cx: &mut TestAppContext) -> Rig {
    let fs = fake();
    let mut state = two_panes(&fs);
    apply(
        &mut state,
        &Command::LayoutPreset {
            tiles: Tiles::builtin()[1].tiles.clone(),
        },
    )
    .expect("three panes");
    state.focus_number(3).expect("third pane");
    apply(
        &mut state,
        &Command::PaneNavigate {
            location: fs.location("dest"),
        },
    )
    .expect("destination");
    state.focus_number(1).expect("first pane");
    let (window, view) = open_options_sized(cx, options_for(fs, state), px(1280.));
    VisualTestContext::from_window(window, cx).simulate_resize(size(px(1280.), px(800.)));
    cx.run_until_parked();
    let panes = view.read_with(cx, |view, _| view.core_layout().unwrap().numbered());
    Rig {
        window,
        view,
        panes,
    }
}

impl Rig {
    fn folders(&self, cx: &TestAppContext) -> Vec<Location> {
        self.view.read_with(cx, |view, _| {
            view.core_layout()
                .unwrap()
                .numbered()
                .iter()
                .map(|pane| {
                    view.browser
                        .state()
                        .pane(*pane)
                        .unwrap()
                        .active()
                        .location()
                        .clone()
                })
                .collect()
        })
    }

    fn origin(&self, cx: &mut TestAppContext, path: bool) -> Point<Pixels> {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            let pane = self.panes[0].get();
            if path {
                window
                    .within(("path", pane))
                    .find(("segment", 0usize))
                    .bounds()
                    .center()
            } else {
                window.within(("tabs", pane)).find(0usize).bounds().center()
            }
        })
        .expect("window")
    }

    fn target(&self, cx: &mut TestAppContext) -> Point<Pixels> {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window.find(("pane", self.panes[2].get())).bounds().center()
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
            ModifiersChangedEvent {
                modifiers: keys,
                capslock: Capslock::default(),
            }
            .to_platform_input(),
        );
        dispatch(cx, self.window, down(MouseButton::Left, from, keys));
        dispatch(cx, self.window, moved(Some(MouseButton::Left), to, keys));
    }

    fn release(&self, cx: &mut TestAppContext, to: Point<Pixels>, keys: Modifiers) {
        dispatch(cx, self.window, up(MouseButton::Left, to, keys));
        settle(cx, &self.view);
    }

    fn has_chip(&self, cx: &mut TestAppContext) -> bool {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window.try_find("pane-swap-chip").is_some()
        })
        .expect("window")
    }
}

#[gpui_kit::test]
fn shift_drag_from_tabs_or_path_swaps_folders_in_both_themes(cx: &mut TestAppContext) {
    for mode in [Mode::Light, Mode::Dark] {
        for path in [false, true] {
            let rig = rig(cx);
            cx.update(|cx| tokens::apply(mode, cx));
            let before = rig.folders(cx);
            let from = rig.origin(cx, path);
            let to = rig.target(cx);
            rig.start(cx, (from, to), shift());
            assert!(rig.has_chip(cx));
            cx.update_window(rig.window, |_, window, _| {
                assert!(
                    window
                        .find(("pane-swap-outline", rig.panes[2].get()))
                        .visible()
                );
            })
            .expect("window");
            rig.release(cx, to, shift());
            assert_eq!(
                rig.folders(cx),
                [before[2].clone(), before[1].clone(), before[0].clone()]
            );
            assert!(!rig.has_chip(cx));
        }
    }
}

#[gpui_kit::test]
fn header_drag_without_shift_does_not_swap(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let before = rig.folders(cx);
    let from = rig.origin(cx, false);
    let to = rig.target(cx);
    rig.start(cx, (from, to), Modifiers::default());
    assert!(!rig.has_chip(cx));
    rig.release(cx, to, Modifiers::default());
    assert_eq!(rig.folders(cx), before);
}

#[gpui_kit::test]
fn escape_cancels_a_pane_drag_before_release(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let before = rig.folders(cx);
    let from = rig.origin(cx, false);
    let to = rig.target(cx);
    rig.start(cx, (from, to), shift());
    assert!(rig.has_chip(cx));
    press(cx, rig.window, &rig.view, "escape");
    assert!(!rig.has_chip(cx));
    rig.release(cx, to, shift());
    assert_eq!(rig.folders(cx), before);
}

#[gpui_kit::test]
fn short_same_pane_and_outside_drags_cancel(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let before = rig.folders(cx);
    let from = rig.origin(cx, false);
    let short = point(from.x + px(5.), from.y);
    rig.start(cx, (from, short), shift());
    assert!(!rig.has_chip(cx));
    rig.release(cx, short, shift());
    for to in [point(from.x + px(6.), from.y), point(px(-20.), px(-20.))] {
        rig.start(cx, (from, to), shift());
        assert!(rig.has_chip(cx));
        rig.release(cx, to, shift());
        assert!(!rig.has_chip(cx));
        assert_eq!(rig.folders(cx), before);
    }
}

#[gpui_kit::test]
fn shift_drag_on_a_file_row_starts_a_file_drag(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let before = rig.folders(cx);
    let from = cx
        .update_window(rig.window, |_, window, cx| {
            window.render_frame(cx);
            window
                .within(("list", rig.panes[0].get()))
                .within(("row", 1usize))
                .find("grip")
                .bounds()
                .center()
        })
        .expect("window");
    let to = point(from.x + px(12.), from.y);
    rig.start(cx, (from, to), shift());
    assert!(!rig.has_chip(cx));
    assert!(
        rig.view
            .read_with(cx, |view, _| view.drag.session.is_some())
    );
    assert_eq!(rig.folders(cx), before);
    press(cx, rig.window, &rig.view, "escape");
}

#[gpui_kit::test]
fn shift_click_in_the_path_field_extends_the_text_selection(cx: &mut TestAppContext) {
    let rig = rig(cx);
    press(cx, rig.window, &rig.view, "alt-d");
    let field = cx
        .update_window(rig.window, |_, window, cx| {
            window.render_frame(cx);
            window.find("path-field").bounds()
        })
        .expect("window");
    let start = point(field.left() + px(2.), field.center().y);
    let end = point(start.x + px(20.), start.y);
    click(cx, rig.window, (start, Modifiers::default()));
    click(cx, rig.window, (end, shift()));
    settle(cx, &rig.view);
    assert!(
        rig.view
            .read_with(cx, |view, _| !view.pane_swap.is_dragging())
    );
    assert!(!rig.has_chip(cx));
    press(cx, rig.window, &rig.view, "ctrl-c");
    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert!(copied.is_some_and(|text| !text.is_empty()));
    assert!(rig.view.read_with(cx, |view, _| view.path_edit.is_some()));
}

#[gpui_kit::test]
fn shift_click_on_a_crumb_still_opens_it_and_starts_no_swap(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let before = rig.folders(cx);
    let from = rig.origin(cx, true);
    click(cx, rig.window, (from, shift()));
    settle(cx, &rig.view);
    assert!(
        !rig.view
            .read_with(cx, |view, _| view.pane_swap.is_dragging())
    );
    assert!(!rig.has_chip(cx));
    let after = rig.folders(cx);
    assert_ne!(after[0], before[0]);
    assert_eq!(after[1..], before[1..]);
}
