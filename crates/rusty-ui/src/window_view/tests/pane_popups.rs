use std::sync::Arc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, ElementId, Entity, TestAppContext, VisualTestContext, Window,
    px, size,
};
use rusty_core::{AppState, Command, Direction, PaneId, apply};
use rusty_fs::FakeFs;

use super::super::keys::{ShowHiddenPath, ShowHiddenTabs};
use super::{open_options_sized, options_for, press, settle};
use crate::window_view::FerriteWindow;

const DEEP: &str = "home/calibration-measurements/powertrain-variants/endurance-exports";
const TABS: usize = 6;
const NARROW: f32 = 480.;
const WIDE: f32 = 1600.;

fn popup_fs() -> Arc<FakeFs> {
    let fs = FakeFs::new();
    for i in 0..5 {
        fs.add_file(
            &format!("{DEEP}/thermal-soak-and-cold-start-log-{i}.csv"),
            4_000,
        );
        fs.add_file(&format!("home/note-{i}.txt"), 10);
    }
    Arc::new(fs)
}

fn popup_state(fs: &FakeFs) -> AppState {
    let mut state = AppState::new(fs.location(DEEP));
    let _ = apply(
        &mut state,
        &Command::LayoutSplit {
            direction: Direction::Right,
        },
    );
    for _ in 1..TABS {
        let _ = apply(&mut state, &Command::PaneOpenTab { location: None });
    }
    state
}

struct Rig {
    window: AnyWindowHandle,
    view: Entity<FerriteWindow>,
    pane: PaneId,
    fs: Arc<FakeFs>,
}

impl Rig {
    fn open(cx: &mut TestAppContext, width: f32) -> Self {
        let fs = popup_fs();
        let state = popup_state(&fs);
        let (window, view) = open_options_sized(cx, options_for(fs.clone(), state), px(width));
        let pane = view.read_with(cx, |view, _| view.browser.state().focused_pane());
        let rig = Self {
            window,
            view,
            pane,
            fs,
        };
        rig.frame(cx, |_, _| {});
        rig
    }

    fn frame<R>(
        &self,
        cx: &mut TestAppContext,
        f: impl FnOnce(&mut Window, &mut gpui_kit::App) -> R,
    ) -> R {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            f(window, cx)
        })
        .expect("window")
    }

    fn scope(&self, bar: &'static str) -> ElementId {
        (bar, self.pane.get()).into()
    }

    fn click(&self, cx: &mut TestAppContext, bar: &'static str, id: &'static str) {
        let scope = self.scope(bar);
        self.frame(cx, |window, cx| window.within(scope).click(id, cx));
    }

    fn has(&self, cx: &mut TestAppContext, bar: &'static str, id: &'static str) -> bool {
        let scope = self.scope(bar);
        self.frame(cx, |window, _| window.within(scope).try_find(id).is_some())
    }

    fn focused(&self, cx: &mut TestAppContext, bar: &'static str, id: &'static str) -> bool {
        let scope = self.scope(bar);
        self.frame(cx, |window, _| {
            window.within(scope).try_find(id).and_then(|e| e.focused()) == Some(true)
        })
    }

    fn list_focused(&self, cx: &mut TestAppContext) -> bool {
        let pane = self.pane;
        let handle = self
            .view
            .read_with(cx, |view, _| view.focus.get(&pane).cloned())
            .expect("list focus handle");
        self.frame(cx, |window, _| handle.is_focused(window))
    }

    fn cursor(&self, cx: &mut TestAppContext) -> u64 {
        self.view.read_with(cx, |view, _| {
            let state = view.browser.state();
            state
                .pane(state.focused_pane())
                .map_or(0, |pane| pane.active().view().cursor)
        })
    }

    fn resize(&self, cx: &mut TestAppContext, width: f32) {
        VisualTestContext::from_window(self.window, cx).simulate_resize(size(px(width), px(600.)));
        cx.run_until_parked();
    }

    fn dispatch(&self, cx: &mut TestAppContext, action: Box<dyn gpui_kit::Action>) {
        self.frame(cx, |window, cx| {
            window.dispatch_action(action, cx);
            window.render_frame(cx);
        });
    }
}

fn widening_closes_and_list_keys_work(
    cx: &mut TestAppContext,
    bar: &'static str,
    ids: [&'static str; 2],
) {
    let rig = Rig::open(cx, NARROW);
    let [anchor, menu] = ids;
    rig.click(cx, bar, anchor);
    assert!(rig.focused(cx, bar, menu), "the {menu} opens with focus");
    rig.resize(cx, WIDE);
    assert!(!rig.has(cx, bar, menu));
    let before = rig.cursor(cx);
    press(cx, rig.window, &rig.view, "down");
    assert_eq!(rig.cursor(cx), before + 1);
}

#[gpui_kit::test]
fn widening_with_the_tab_menu_open_gives_the_keys_back_to_the_list(cx: &mut TestAppContext) {
    widening_closes_and_list_keys_work(cx, "tabs", ["tab-more", "tab-more-menu"]);
}

#[gpui_kit::test]
fn widening_with_the_path_menu_open_gives_the_keys_back_to_the_list(cx: &mut TestAppContext) {
    widening_closes_and_list_keys_work(cx, "path", ["path-more", "path-hidden-menu"]);
}

#[gpui_kit::test]
fn a_path_menu_whose_crumb_goes_away_gives_the_keys_back_to_the_list(cx: &mut TestAppContext) {
    let rig = Rig::open(cx, NARROW);
    rig.click(cx, "path", "path-more");
    assert!(rig.focused(cx, "path", "path-hidden-menu"));
    let location = rig.fs.location("home");
    rig.view.update(cx, |view, cx| {
        let _ = view.browser.command(&Command::PaneNavigate { location });
        cx.notify();
    });
    settle(cx, &rig.view);
    assert!(!rig.has(cx, "path", "path-more"));
    assert!(rig.list_focused(cx));
    let before = rig.cursor(cx);
    press(cx, rig.window, &rig.view, "down");
    assert_eq!(rig.cursor(cx), before + 1);
}

#[gpui_kit::test]
fn the_show_hidden_commands_open_the_menus_with_focus(cx: &mut TestAppContext) {
    let rig = Rig::open(cx, NARROW);
    rig.dispatch(cx, Box::new(ShowHiddenTabs));
    assert!(rig.focused(cx, "tabs", "tab-more-menu"));
    press(cx, rig.window, &rig.view, "escape");
    assert!(rig.list_focused(cx));
    rig.dispatch(cx, Box::new(ShowHiddenPath));
    assert!(rig.focused(cx, "path", "path-hidden-menu"));
    press(cx, rig.window, &rig.view, "escape");
    assert!(rig.list_focused(cx));
}

#[gpui_kit::test]
fn the_show_hidden_commands_do_nothing_when_nothing_is_hidden(cx: &mut TestAppContext) {
    let rig = Rig::open(cx, WIDE);
    rig.dispatch(cx, Box::new(ShowHiddenTabs));
    rig.dispatch(cx, Box::new(ShowHiddenPath));
    assert!(!rig.has(cx, "tabs", "tab-more-menu"));
    assert!(!rig.has(cx, "path", "path-hidden-menu"));
    assert!(rig.list_focused(cx));
}

fn focus_anchor(rig: &Rig, cx: &mut TestAppContext, tabs: bool) {
    let pane = rig.pane;
    let anchor = rig.view.read_with(cx, |view, cx| {
        let chrome = view.chrome.get(pane).expect("pane chrome");
        let menu = if tabs { &chrome.tabs } else { &chrome.path };
        menu.read(cx).anchor().clone()
    });
    rig.frame(cx, |window, cx| window.focus(&anchor, cx));
}

#[gpui_kit::test]
fn enter_and_space_open_the_focused_more_buttons(cx: &mut TestAppContext) {
    let rig = Rig::open(cx, NARROW);
    focus_anchor(&rig, cx, true);
    assert!(rig.focused(cx, "tabs", "tab-more"));
    press(cx, rig.window, &rig.view, "enter");
    assert!(rig.focused(cx, "tabs", "tab-more-menu"));
    press(cx, rig.window, &rig.view, "escape");
    assert!(rig.focused(cx, "tabs", "tab-more"));
    focus_anchor(&rig, cx, false);
    press(cx, rig.window, &rig.view, "space");
    assert!(rig.focused(cx, "path", "path-hidden-menu"));
}

#[gpui_kit::test]
fn the_more_buttons_are_in_the_tab_order(cx: &mut TestAppContext) {
    let rig = Rig::open(cx, NARROW);
    focus_anchor(&rig, cx, true);
    rig.frame(cx, |window, cx| window.focus_next(cx));
    assert!(rig.focused(cx, "path", "path-more"));
}

fn header_and_crumbs(window: &mut Window, pane: PaneId) -> ([bool; 3], usize) {
    let list = window.within(("list", pane.get()));
    let columns = [1usize, 3, 2].map(|k| list.try_find(("sort", k)).is_some());
    let path = window.within(("path", pane.get()));
    let crumbs = (0..8usize)
        .filter(|i| path.try_find(("segment", *i)).is_some())
        .count();
    (columns, crumbs)
}

#[gpui_kit::test]
fn the_first_frame_after_tab_in_a_narrow_window_is_already_fitted(cx: &mut TestAppContext) {
    let rig = Rig::open(cx, 400.);
    let view = rig.view.clone();
    let (pane, first) = cx
        .update_window(rig.window, |_, window, cx| {
            view.update(cx, |view, cx| view.switch_pane(window, cx));
            window.render_frame(cx);
            let pane = view.read(cx).browser.state().focused_pane();
            (pane, header_and_crumbs(window, pane))
        })
        .expect("window");
    let settled = rig.frame(cx, |window, cx| {
        window.render_frame(cx);
        header_and_crumbs(window, pane)
    });
    assert_eq!(
        first, settled,
        "first frame (Ext, Date, Size; crumbs) differs from the settled one"
    );
    assert!(!settled.0[0], "Ext still shows at 400 px");
    assert!(settled.1 > 0, "the path bar shows no crumbs");
}
