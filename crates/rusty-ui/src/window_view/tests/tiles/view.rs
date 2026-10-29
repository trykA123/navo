use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, MouseButton, TestAppContext, VisualTestContext,
    point, px, size,
};
use rusty_core::{Command, LayoutState, SplitPath, Tiles, apply};

use super::super::{fake, open_options_sized, options_for, press, two_panes};
use crate::tokens::{self, Mode};
use crate::window_view::FerriteWindow;

fn open_tiles(cx: &mut TestAppContext, preset: usize) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let fs = fake();
    let mut state = two_panes(&fs);
    apply(
        &mut state,
        &Command::LayoutPreset {
            tiles: Tiles::builtin()[preset].tiles.clone(),
        },
    )
    .expect("preset");
    let opened = open_options_sized(cx, options_for(fs, state), px(1280.));
    VisualTestContext::from_window(opened.0, cx).simulate_resize(size(px(1280.), px(800.)));
    cx.run_until_parked();
    opened
}

fn session_ratio(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> u16 {
    view.read_with(cx, |view, _| {
        let session = view.browser.state().to_session(30);
        let LayoutState::Split { ratio, .. } = session.windows[0].layout else {
            panic!("root split");
        };
        ratio
    })
}

#[gpui_kit::test]
fn three_and_four_panes_follow_core_rectangles(cx: &mut TestAppContext) {
    for preset in 1..5 {
        let (window, view) = open_tiles(cx, preset);
        for mode in [Mode::Light, Mode::Dark] {
            cx.update(|cx| tokens::apply(mode, cx));
            let rects = view.read_with(cx, |view, _| {
                let state = view.browser.state();
                state
                    .window(state.focused_window())
                    .unwrap()
                    .layout()
                    .rects()
            });
            cx.update_window(window, |_, window, cx| {
                window.render_frame(cx);
                window.render_frame(cx);
                let area = window.find("tiles").bounds();
                let gap = tokens::current(cx).spacing.xs;
                for (pane, rect) in &rects {
                    let card = window.find(("pane", pane.get())).bounds();
                    let expected = [
                        area.left() + area.size.width * rect.x as f32 + gap / 2.,
                        area.top() + area.size.height * rect.y as f32 + gap / 2.,
                        area.size.width * rect.width as f32 - gap,
                        area.size.height * rect.height as f32 - gap,
                    ];
                    for (actual, expected) in
                        [card.left(), card.top(), card.size.width, card.size.height]
                            .into_iter()
                            .zip(expected)
                    {
                        assert!(
                            (actual - expected).abs() <= px(1.),
                            "preset {preset}: {actual:?} != {expected:?}"
                        );
                    }
                }
            })
            .expect("window");
        }
    }
}

#[gpui_kit::test]
fn dragging_previews_then_saves_the_root_ratio_and_double_click_evens_it(cx: &mut TestAppContext) {
    let (window, view) = open_tiles(cx, 0);
    let (from, to) = cx
        .update_window(window, |_, window, cx| {
            window.render_frame(cx);
            let area = window.find("tiles").bounds();
            let from = window.find("divider-root").bounds().center();
            (from, point(area.left() + area.size.width * 0.7, from.y))
        })
        .expect("window");
    let mut visual = VisualTestContext::from_window(window, cx);
    visual.simulate_mouse_down(from, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(to, MouseButton::Left, Default::default());
    assert_eq!(session_ratio(cx, &view), 500);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!((window.find("divider-root").bounds().center().x - to.x).abs() <= px(1.));
    })
    .expect("window");
    VisualTestContext::from_window(window, cx).simulate_mouse_up(
        to,
        MouseButton::Left,
        Default::default(),
    );
    assert_eq!(session_ratio(cx, &view), 700);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.double_click("divider-root", cx);
    })
    .expect("window");
    assert_eq!(session_ratio(cx, &view), 500);
}

fn first_pane_width(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    pane: u64,
) -> gpui_kit::Pixels {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.find(("pane", pane)).bounds().size.width
    })
    .expect("window")
}

#[gpui_kit::test]
fn a_drag_moves_only_the_divider_and_resizes_the_panes_on_release(cx: &mut TestAppContext) {
    let (window, view) = open_tiles(cx, 0);
    let first = view.read_with(cx, |view, _| view.panes()[0].get());
    let before = first_pane_width(cx, window, first);
    let (from, to) = cx
        .update_window(window, |_, window, _| {
            let area = window.find("tiles").bounds();
            let from = window.find("divider-root").bounds().center();
            (from, point(area.left() + area.size.width * 0.7, from.y))
        })
        .expect("window");
    let mut visual = VisualTestContext::from_window(window, cx);
    visual.simulate_mouse_down(from, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(to, MouseButton::Left, Default::default());
    assert!((first_pane_width(cx, window, first) - before).abs() <= px(1.));
    cx.update_window(window, |_, window, _| {
        assert!((window.find("divider-root").bounds().center().x - to.x).abs() <= px(1.));
    })
    .expect("window");
    VisualTestContext::from_window(window, cx).simulate_mouse_up(
        to,
        MouseButton::Left,
        Default::default(),
    );
    assert!(first_pane_width(cx, window, first) > before + px(100.));
}

#[gpui_kit::test]
fn a_drag_stops_at_the_last_ratio_that_keeps_every_pane_visible(cx: &mut TestAppContext) {
    let (window, view) = open_tiles(cx, 0);
    let (from, fits, too_narrow) = cx
        .update_window(window, |_, window, cx| {
            window.render_frame(cx);
            let area = window.find("tiles").bounds();
            let from = window.find("divider-root").bounds().center();
            let at = |share: f32| point(area.left() + area.size.width * share, from.y);
            (from, at(0.7), at(0.1))
        })
        .expect("window");
    let mut visual = VisualTestContext::from_window(window, cx);
    visual.simulate_mouse_down(from, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(fits, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(too_narrow, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(too_narrow, MouseButton::Left, Default::default());
    assert_eq!(session_ratio(cx, &view), 700);
    let panes = view.read_with(cx, |view, _| view.panes().len());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(view.read(cx).shown_panes(window).len(), panes);
    })
    .expect("window");
}

#[gpui_kit::test]
fn an_undersized_layout_shows_only_the_focused_pane(cx: &mut TestAppContext) {
    let (window, view) = open_tiles(cx, 1);
    view.update(cx, |view, cx| {
        view.browser
            .command(&Command::LayoutRatio {
                split: SplitPath(Vec::new()),
                ratio: 100,
            })
            .unwrap();
        cx.notify();
    });
    let (panes, focused) = view.read_with(cx, |view, _| {
        (view.panes(), view.browser.state().focused_pane())
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        for pane in panes {
            assert_eq!(
                window.try_find(("pane", pane.get())).is_some(),
                pane == focused
            );
        }
        assert!(window.try_find("divider-root").is_none());
    })
    .expect("window");
}

fn split_ratios(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> Vec<(String, u16)> {
    view.read_with(cx, |view, _| {
        view.core_layout()
            .unwrap()
            .splits()
            .into_iter()
            .map(|split| (split.path.to_string(), split.ratio))
            .collect()
    })
}

fn pane_widths(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<FerriteWindow>,
) -> Vec<gpui_kit::Pixels> {
    let panes = view.read_with(cx, |view, _| view.tiles_layout().panes());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        panes
            .iter()
            .map(|pane| window.find(("pane", pane.get())).bounds().size.width)
            .collect()
    })
    .expect("window")
}

#[gpui_kit::test]
fn each_row_of_a_two_by_two_grid_has_its_own_divider(cx: &mut TestAppContext) {
    let (window, view) = open_tiles(cx, 3);
    let before = pane_widths(cx, window, &view);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let area = window.find("tiles").bounds();
        let from = window.find("divider-1").bounds().center();
        let to = point(area.left() + area.size.width * 0.7, from.y);
        window.drag(from, to, cx);
    })
    .expect("window");
    assert_eq!(
        split_ratios(cx, &view),
        [
            ("root".to_owned(), 500),
            ("1".to_owned(), 700),
            ("2".to_owned(), 500)
        ]
    );
    let after = pane_widths(cx, window, &view);
    assert!(after[0] > before[0] + px(100.));
    assert!(after[1] < before[1] - px(100.));
    assert!((after[2] - before[2]).abs() <= px(1.));
    assert!((after[3] - before[3]).abs() <= px(1.));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.double_click("divider-1", cx);
    })
    .expect("window");
    assert!(
        split_ratios(cx, &view)
            .iter()
            .all(|(_, ratio)| *ratio == 500)
    );
}

#[gpui_kit::test]
fn ctrl_alt_zero_resets_both_row_dividers_of_a_grid(cx: &mut TestAppContext) {
    let (window, view) = open_tiles(cx, 3);
    view.update(cx, |view, cx| {
        for (path, ratio) in [("1", 300), ("2", 700), ("root", 400)] {
            view.browser
                .command(&Command::LayoutRatio {
                    split: path.parse().unwrap(),
                    ratio,
                })
                .unwrap();
        }
        cx.notify();
    });
    press(cx, window, &view, "ctrl-alt-0");
    assert!(
        split_ratios(cx, &view)
            .iter()
            .all(|(_, ratio)| *ratio == 500)
    );
}

#[gpui_kit::test]
fn an_undersized_pane_height_also_uses_one_pane_and_restores_the_layout(cx: &mut TestAppContext) {
    let (window, view) = open_tiles(cx, 3);
    for (height, expected) in [(320., 1), (800., 4)] {
        VisualTestContext::from_window(window, cx).simulate_resize(size(px(1280.), px(height)));
        cx.run_until_parked();
        let panes = view.read_with(cx, |view, _| view.panes());
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            window.render_frame(cx);
            let count = panes
                .into_iter()
                .filter(|pane| window.try_find(("pane", pane.get())).is_some())
                .count();
            assert_eq!(count, expected);
        })
        .expect("window");
    }
}
