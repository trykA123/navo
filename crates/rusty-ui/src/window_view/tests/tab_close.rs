use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, Modifiers, MouseButton, Point, TestAppContext, px,
};
use rusty_core::{AppState, Command, PaneId, apply};

use super::{fake, open_options_sized, options_for, press, settle, two_panes};
use crate::test_pointer::{click, dispatch, down};
use crate::window_view::FerriteWindow;

struct Rig {
    window: AnyWindowHandle,
    view: Entity<FerriteWindow>,
    left: PaneId,
    right: PaneId,
}

fn rig(cx: &mut TestAppContext, tabs: [usize; 2]) -> Rig {
    let fs = fake();
    let mut state = two_panes(&fs);
    for (number, count) in [(2, tabs[1]), (1, tabs[0])] {
        state.focus_number(number).expect("pane");
        for _ in 1..count {
            apply(&mut state, &Command::PaneOpenTab { location: None }).expect("tab");
        }
    }
    let (window, view) = open_options_sized(cx, options_for(fs, state), px(1280.));
    cx.run_until_parked();
    let panes = view.read_with(cx, |view, _| view.panes());
    Rig {
        window,
        view,
        left: panes[0],
        right: panes[1],
    }
}

impl Rig {
    fn tab_counts(&self, cx: &TestAppContext) -> Vec<usize> {
        self.view.read_with(cx, |view, _| {
            let state = view.browser.state();
            view.panes()
                .into_iter()
                .map(|pane| state.pane(pane).expect("pane").tabs().len())
                .collect()
        })
    }

    fn close_button(
        &self,
        cx: &mut TestAppContext,
        pane: PaneId,
        index: usize,
    ) -> Point<gpui_kit::Pixels> {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window
                .within(("tabs", pane.get()))
                .within(index)
                .find(("tab-close", index))
                .bounds()
                .center()
        })
        .expect("window")
    }

    fn close_with_x(&self, cx: &mut TestAppContext, pane: PaneId, index: usize) {
        let at = self.close_button(cx, pane, index);
        click(cx, self.window, (at, Modifiers::default()));
        settle(cx, &self.view);
    }
}

#[gpui_kit::test]
fn the_x_on_a_tab_closes_it(cx: &mut TestAppContext) {
    let rig = rig(cx, [3, 2]);
    rig.close_with_x(cx, rig.left, 1);
    assert_eq!(rig.tab_counts(cx), [2, 2]);
    assert_eq!(rig.view.read_with(cx, |view, _| view.panes().len()), 2);
}

#[gpui_kit::test]
fn a_middle_click_on_a_tab_closes_it(cx: &mut TestAppContext) {
    let rig = rig(cx, [3, 2]);
    let at = cx
        .update_window(rig.window, |_, window, cx| {
            window.render_frame(cx);
            window
                .within(("tabs", rig.right.get()))
                .find(1usize)
                .bounds()
                .center()
        })
        .expect("window");
    dispatch(
        cx,
        rig.window,
        down(MouseButton::Middle, at, Modifiers::default()),
    );
    settle(cx, &rig.view);
    assert_eq!(rig.tab_counts(cx), [3, 1]);
}

#[gpui_kit::test]
fn closing_the_last_tab_of_one_of_two_panes_closes_that_pane(cx: &mut TestAppContext) {
    let rig = rig(cx, [1, 2]);
    rig.close_with_x(cx, rig.right, 1);
    assert_eq!(rig.tab_counts(cx), [1, 1]);
    press(cx, rig.window, &rig.view, "ctrl-w");
    assert_eq!(rig.tab_counts(cx), [1]);
}

#[gpui_kit::test]
fn closing_the_last_tab_of_the_only_pane_leaves_one_fresh_tab(cx: &mut TestAppContext) {
    let fs = fake();
    let state = AppState::new(fs.location("home"));
    let (window, view) = open_options_sized(cx, options_for(fs.clone(), state), px(1280.));
    let first = view.read_with(cx, |view, _| view.browser.state().active_tab().id());
    press(cx, window, &view, "ctrl-w");
    let (panes, tabs, now, folder) = view.read_with(cx, |view, _| {
        let tab = view.browser.state().active_tab();
        let pane = view.browser.state().focused_pane();
        let count = view.browser.state().pane(pane).map(|p| p.tabs().len());
        (view.panes().len(), count, tab.id(), tab.location().clone())
    });
    assert_eq!((panes, tabs), (1, Some(1)));
    assert_ne!(now, first);
    assert_eq!(folder, fs.location("home"));
}
