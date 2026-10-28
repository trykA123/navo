use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, TestAppContext, VisualTestContext, Window, px, size,
};
use rusty_core::{AppState, PaneId};
use rusty_fs::FakeFs;

use super::super::{fake, open_options_sized, options_for, two_panes};
use super::three_panes;
use crate::tokens::{self, Mode};
use crate::window_view::FerriteWindow;

fn open_wide(
    cx: &mut TestAppContext,
    pick: fn(&FakeFs) -> AppState,
) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let fs = fake();
    let state = pick(&fs);
    let opened = open_options_sized(cx, options_for(fs, state), px(1280.));
    VisualTestContext::from_window(opened.0, cx).simulate_resize(size(px(1280.), px(800.)));
    cx.run_until_parked();
    opened
}

fn roles(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> (Vec<PaneId>, PaneId, PaneId) {
    view.read_with(cx, |view, _| {
        let target = view.browser.target_pane().expect("target");
        let panes = view.core_layout().expect("layout").numbered();
        (panes, view.browser.state().focused_pane(), target)
    })
}

fn centre_y(window: &Window, id: (&'static str, u64)) -> f32 {
    f32::from(window.find(id).bounds().center().y)
}

#[gpui_kit::test]
fn three_panes_show_numbers_the_active_tag_and_the_target_tag(cx: &mut TestAppContext) {
    let (window, view) = open_wide(cx, three_panes);
    let (panes, focused, target) = roles(cx, &view);
    for mode in [Mode::Light, Mode::Dark] {
        cx.update(|cx| tokens::apply(mode, cx));
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            window.render_frame(cx);
            for (index, pane) in panes.iter().enumerate() {
                let id = pane.get();
                assert!(
                    window.find(("pane-number", id)).visible(),
                    "pane {}",
                    index + 1
                );
                let has = |name: &'static str| window.try_find((name, id)).is_some();
                assert_eq!(has("pane-tag-active"), *pane == focused);
                assert_eq!(has("pane-tag-target"), *pane == target);
                assert_eq!(has("pane-target-outline"), *pane == target);
            }
            for (pane, tag) in [(focused, "pane-tag-active"), (target, "pane-tag-target")] {
                let id = pane.get();
                let line = centre_y(window, ("pane-number", id));
                assert!((centre_y(window, (tag, id)) - line).abs() <= 1., "{tag}");
                let tab = window.within(("tabs", id)).find(0usize);
                assert!((f32::from(tab.bounds().center().y) - line).abs() <= 1.);
            }
        })
        .expect("window");
    }
}

#[gpui_kit::test]
fn two_panes_show_no_numbers_or_tags_but_outline_the_target(cx: &mut TestAppContext) {
    let (window, view) = open_wide(cx, two_panes);
    let (panes, _, target) = roles(cx, &view);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        for pane in &panes {
            let id = pane.get();
            assert!(window.try_find(("pane-number", id)).is_none());
            assert!(window.try_find(("pane-tag-active", id)).is_none());
            assert!(window.try_find(("pane-tag-target", id)).is_none());
            let outline = window.try_find(("pane-target-outline", id));
            assert_eq!(outline.is_some(), *pane == target);
        }
    })
    .expect("window");
}

#[gpui_kit::test]
fn one_pane_mode_shows_no_markers_on_the_shown_pane(cx: &mut TestAppContext) {
    let (window, view) = open_wide(cx, three_panes);
    VisualTestContext::from_window(window, cx).simulate_resize(size(px(800.), px(800.)));
    cx.run_until_parked();
    let (_, focused, _) = roles(cx, &view);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let id = focused.get();
        assert!(window.try_find(("pane-number", id)).is_none());
        assert!(window.try_find(("pane-tag-active", id)).is_none());
    })
    .expect("window");
}
