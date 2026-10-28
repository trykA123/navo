use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Entity, Pixels, TestAppContext, VisualTestContext, px,
};
use rusty_core::Command;

use super::super::fkeys::hold;
use super::super::press;
use super::{ALT, rig, shape, tray, wait};
use crate::window_view::FerriteWindow;

const FOOT_IDS: [&str; 7] = [
    "workspace-draft",
    "workspace-save",
    "workspace-discard",
    "workspace-rename",
    "workspace-move-left",
    "workspace-move-right",
    "workspace-delete",
];

fn resize(cx: &mut TestAppContext, window: AnyWindowHandle, width: f32) {
    VisualTestContext::from_window(window, cx).simulate_resize(gpui_kit::size(px(width), px(700.)));
}

fn show(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) {
    cx.update(|cx| cx.set_reduce_motion(true));
    view.update(cx, |view, cx| {
        view.workspace_keys.visible = true;
        cx.notify();
    });
}

fn inside(outer: Bounds<Pixels>, inner: Bounds<Pixels>) -> bool {
    inner.left() >= outer.left()
        && inner.top() >= outer.top()
        && inner.right() <= outer.right()
        && inner.bottom() <= outer.bottom()
}

fn notice(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> Option<String> {
    view.read_with(cx, |view, _| {
        view.status_notice().map(|notice| notice.text.to_string())
    })
}

fn workspace_names(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> Vec<String> {
    view.read_with(cx, |view, _| {
        let state = view.browser.state();
        let core = state.window(state.focused_window()).expect("core window");
        core.workspaces().iter().map(|w| w.name.clone()).collect()
    })
}

#[gpui_kit::test]
fn the_hints_drawer_stays_hidden_while_the_tray_shows(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    hold(cx, window, ALT);
    wait(cx, 1000);
    assert!(tray(cx, &view));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("workspace-tray").visible());
        assert!(window.try_find("modifier-hints").is_none());
    })
    .expect("window");
}

#[gpui_kit::test]
fn card_feet_fit_inside_cards_of_one_height_with_a_draft(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    press(cx, window, &view, "ctrl-t");
    show(cx, &view);
    for width in [1920., 390.] {
        resize(cx, window, width);
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find(("workspace-draft", 1usize)).is_some());
            let new = window.find("workspace-new").bounds();
            for n in 1..=2usize {
                let card = window.find(("workspace-card", n)).bounds();
                assert_eq!(card.size.height, new.size.height, "card {n} at {width}");
                assert_eq!(card.top(), new.top(), "card {n} at {width}");
                for id in FOOT_IDS {
                    if let Some(part) = window.try_find((id, n)) {
                        let part = part.bounds();
                        assert!(
                            inside(card, part),
                            "{id} {n} at {width}: {part:?} in {card:?}"
                        );
                    }
                }
            }
        })
        .expect("window");
    }
}

#[gpui_kit::test]
fn the_tray_content_has_one_inset_and_the_header_lines_up_with_the_cards(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    view.update(cx, |view, cx| {
        for _ in 0..7 {
            view.browser
                .command(&Command::WorkspaceNew { name: None })
                .expect("new workspace");
        }
        view.browser
            .command(&Command::WorkspaceSwitch { n: 1 })
            .expect("first workspace");
        cx.notify();
    });
    show(cx, &view);
    resize(cx, window, 1920.);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let tray = window.find("workspace-tray").bounds();
        let header = window.find("workspace-tray-header").bounds();
        let title = window.find("workspace-tray-title").bounds();
        let cards = window.find("workspace-cards").bounds();
        let first = window.find(("workspace-card", 1usize)).bounds();
        let inset = header.left() - tray.left();
        assert!(inset > px(0.));
        assert_eq!(tray.right() - header.right(), inset);
        let top = header.top() - tray.top() - inset;
        assert!(top >= px(0.) && top <= px(1.), "top border {top:?}");
        assert_eq!(tray.bottom() - cards.bottom(), inset);
        assert_eq!(title.left(), first.left());
        assert!(
            window
                .within("workspace-tray")
                .find(("workspace-card", 9usize))
                .visible()
        );
    })
    .expect("window");
}

#[gpui_kit::test]
fn alt_n_and_the_new_card_show_the_created_notice(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    hold(cx, window, ALT);
    press(cx, window, &view, "alt-n");
    assert_eq!(shape(cx, &view), (2, 3));
    let name = workspace_names(cx, &view)[2].clone();
    let text = notice(cx, &view).expect("notice after Alt+N");
    assert!(text.contains(&name) && text.contains('3'), "{text}");
    hold(cx, window, gpui_kit::Modifiers::default());
    show(cx, &view);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("workspace-new-switch", cx);
    })
    .expect("window");
    assert_eq!(shape(cx, &view), (3, 4));
    let name = workspace_names(cx, &view)[3].clone();
    let text = notice(cx, &view).expect("notice after the New card");
    assert!(text.contains(&name) && text.contains('4'), "{text}");
}
