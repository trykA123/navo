use std::rc::Rc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, Modifiers, TestAppContext};
use rusty_core::{Command, apply};

use super::fkeys::hold;
use super::{FerriteWindow, fake, open_with, press, two_panes};

mod actions;
mod button;

const ALT: Modifiers = Modifiers {
    alt: true,
    control: false,
    shift: false,
    platform: false,
    function: false,
};

fn rig(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let fs = fake();
    let mut state = two_panes(&fs);
    apply(&mut state, &Command::WorkspaceSave).expect("save initial layout");
    apply(&mut state, &Command::WorkspaceNew { name: None }).expect("new workspace");
    apply(&mut state, &Command::WorkspaceSwitch { n: 1 }).expect("first workspace");
    let (window, view) = open_with(cx, fs, state, Rc::default());
    hold(cx, window, Modifiers::default());
    (window, view)
}

fn three_workspaces(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let (window, view) = rig(cx);
    view.update(cx, |view, cx| {
        view.browser
            .command(&Command::WorkspaceNew { name: None })
            .expect("third workspace");
        view.browser
            .command(&Command::WorkspaceSwitch { n: 1 })
            .expect("first workspace");
        cx.notify();
    });
    hold(cx, window, ALT);
    press(cx, window, &view, "alt-2");
    (window, view)
}

#[gpui_kit::test]
fn three_workspaces_render_three_cards_and_new(cx: &mut TestAppContext) {
    let (window, _) = three_workspaces(cx);
    cx.update(|cx| cx.set_reduce_motion(true));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("workspace-tray").bounds().size.width,
            window.viewport_size().width
        );
        let tray = window.within("workspace-tray");
        for n in 1..=3usize {
            assert!(tray.find(("workspace-card", n)).visible());
        }
        assert!(tray.try_find(("workspace-card", 4usize)).is_none());
        assert!(tray.find("workspace-new").visible());
    })
    .expect("window");
    gpui_kit::VisualTestContext::from_window(window, cx)
        .simulate_resize(gpui_kit::size(gpui_kit::px(390.), gpui_kit::px(600.)));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let first = window.find(("workspace-card", 1usize));
        assert!(first.visible());
        assert!(first.bounds().size.width < window.viewport_size().width);
        let before = window.find(("workspace-card", 3usize)).bounds().origin.x;
        window.scroll(
            "workspace-cards",
            gpui_kit::ScrollDelta::Pixels(gpui_kit::point(gpui_kit::px(-600.), gpui_kit::px(0.))),
            cx,
        );
        let after = window.find(("workspace-card", 3usize)).bounds().origin.x;
        assert!(after < before);
    })
    .expect("window");
}

#[gpui_kit::test]
fn the_active_workspace_card_says_open(cx: &mut TestAppContext) {
    let (window, _) = three_workspaces(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let mut tray = window.within("workspace-tray");
        assert_eq!(
            tray.within(("workspace-card", 1usize))
                .find("workspace-open")
                .label(),
            Some(rusty_l10n::tr("workspace-open").as_str())
        );
        assert!(
            tray.within(("workspace-card", 2usize))
                .try_find("workspace-open")
                .is_none()
        );
    })
    .expect("window");
}

#[gpui_kit::test]
fn alt_two_marks_the_second_card_and_shows_the_title_ribbon(cx: &mut TestAppContext) {
    let (window, view) = three_workspaces(cx);
    let expected = view.read_with(cx, |view, _| {
        let state = view.browser.state();
        let core = state.window(state.focused_window()).expect("core window");
        rusty_l10n::t!(
            "workspace-preview-ribbon",
            name = core.workspaces()[1].name.clone()
        )
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let tray = window.within("workspace-tray");
        assert_eq!(
            tray.find(("workspace-card", 1usize)).selected(),
            Some(false)
        );
        assert_eq!(tray.find(("workspace-card", 2usize)).selected(), Some(true));
        let ribbon = window.find("workspace-preview-ribbon");
        assert!(ribbon.visible());
        assert_eq!(ribbon.label(), Some(expected.as_str()));
    })
    .expect("window");
}

#[gpui_kit::test]
fn clicking_the_third_workspace_switches_and_closes_the_tray(cx: &mut TestAppContext) {
    let (window, view) = three_workspaces(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .within("workspace-tray")
            .click(("workspace-card", 3usize), cx);
    })
    .expect("window");
    assert_eq!(shape(cx, &view), (2, 3));
    assert!(!tray(cx, &view));
    hold(cx, window, Modifiers::default());
    assert_eq!(shape(cx, &view), (2, 3));
}

fn shape(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> (usize, usize) {
    view.read_with(cx, |view, _| {
        let state = view.browser.state();
        let window = state.window(state.focused_window()).expect("core window");
        (window.active_workspace(), window.workspaces().len())
    })
}

#[gpui_kit::test]
fn alt_number_switches_only_when_alt_is_released(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    hold(cx, window, ALT);
    press(cx, window, &view, "alt-2");
    assert_eq!(shape(cx, &view), (0, 2));
    hold(cx, window, Modifiers::default());
    assert_eq!(shape(cx, &view), (1, 2));
}

#[gpui_kit::test]
fn escape_cancels_the_preview_until_alt_is_released(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    hold(cx, window, ALT);
    press(cx, window, &view, "alt-2");
    press(cx, window, &view, "alt-escape");
    hold(cx, window, Modifiers::default());
    assert_eq!(shape(cx, &view), (0, 2));
}

#[gpui_kit::test]
fn a_number_beyond_the_list_appends_one_workspace_on_release(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    hold(cx, window, ALT);
    press(cx, window, &view, "alt-9");
    assert_eq!(shape(cx, &view), (0, 2));
    hold(cx, window, Modifiers::default());
    assert_eq!(shape(cx, &view), (2, 3));
}

fn tray(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> bool {
    view.read_with(cx, |view, _| view.workspace_keys.visible)
}

fn wait(cx: &mut TestAppContext, ms: u64) {
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(ms));
    cx.run_until_parked();
}

#[gpui_kit::test]
fn alt_alone_opens_the_tray_after_the_configured_hint_delay(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    view.update(cx, |view, _| {
        view.values = view.values.with(&crate::settings::Change {
            field: crate::settings::Field::ModifierHintsDelay,
            value: crate::settings::Value::Number(600),
        });
    });
    hold(cx, window, ALT);
    wait(cx, 599);
    assert!(!tray(cx, &view));
    wait(cx, 1);
    assert!(tray(cx, &view));
    hold(cx, window, Modifiers::default());
    assert!(!tray(cx, &view));
    assert_eq!(shape(cx, &view), (0, 2));
}

#[gpui_kit::test]
fn a_shortcut_before_the_delay_does_not_open_the_tray(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    hold(cx, window, ALT);
    press(cx, window, &view, "alt-left");
    wait(cx, 1000);
    assert!(!tray(cx, &view));
}

#[gpui_kit::test]
fn number_keys_preview_immediately_and_the_active_workspace_clears_the_preview(
    cx: &mut TestAppContext,
) {
    use crate::window_view::workspace_keys::Preview;

    let (window, view) = rig(cx);
    hold(cx, window, ALT);
    press(cx, window, &view, "alt-2");
    assert!(tray(cx, &view));
    assert_eq!(
        view.read_with(cx, |view, _| view.workspace_keys.preview),
        Some(Preview::Existing(2))
    );
    press(cx, window, &view, "alt-9");
    assert_eq!(
        view.read_with(cx, |view, _| view.workspace_keys.preview),
        Some(Preview::New)
    );
    press(cx, window, &view, "alt-1");
    assert_eq!(
        view.read_with(cx, |view, _| view.workspace_keys.preview),
        None
    );
    hold(cx, window, Modifiers::default());
    assert_eq!(shape(cx, &view), (0, 2));
}

#[gpui_kit::test]
fn losing_window_focus_cancels_a_preview(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    hold(cx, window, ALT);
    press(cx, window, &view, "alt-2");
    let mut visual = gpui_kit::VisualTestContext::from_window(window, cx);
    visual.deactivate_window();
    visual.run_until_parked();
    visual.simulate_modifiers_change(Modifiers::default());
    visual.run_until_parked();
    assert!(!tray(cx, &view));
    assert_eq!(shape(cx, &view), (0, 2));
}

#[gpui_kit::test]
fn alt_release_switches_even_while_the_number_key_is_down(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    hold(cx, window, ALT);
    let mut visual = gpui_kit::VisualTestContext::from_window(window, cx);
    visual.simulate_event(gpui_kit::KeyDownEvent {
        keystroke: gpui_kit::Keystroke::parse("alt-2").expect("keystroke"),
        is_held: false,
        prefer_character_input: false,
    });
    visual.run_until_parked();
    assert_eq!(shape(cx, &view), (0, 2));
    hold(cx, window, Modifiers::default());
    assert_eq!(shape(cx, &view), (1, 2));
}

#[gpui_kit::test]
fn alt_left_and_right_keep_back_and_forward(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    let location = fake().location("home/docs");
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.apply(&Command::PaneNavigate { location }, window, cx)
        });
    })
    .expect("window");
    super::settle(cx, &view);
    hold(cx, window, ALT);
    press(cx, window, &view, "alt-left");
    assert_eq!(
        view.read_with(cx, |view, _| view
            .browser
            .state()
            .active_tab()
            .location()
            .clone()),
        fake().location("home")
    );
    press(cx, window, &view, "alt-right");
    assert_eq!(
        view.read_with(cx, |view, _| view
            .browser
            .state()
            .active_tab()
            .location()
            .clone()),
        fake().location("home/docs")
    );
    hold(cx, window, Modifiers::default());
    assert_eq!(shape(cx, &view), (0, 2));
}

#[gpui_kit::test]
fn ctrl_s_saves_the_active_workspace_draft(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    press(cx, window, &view, "ctrl-t");
    let changes = |cx: &mut TestAppContext| {
        view.read_with(cx, |view, _| {
            let state = view.browser.state();
            let core = state.window(state.focused_window()).expect("core window");
            rusty_core::changes(
                &core.workspaces()[core.active_workspace()].saved,
                &core.snapshot(),
            )
        })
    };
    assert_eq!(changes(cx), 1);
    press(cx, window, &view, "ctrl-s");
    assert_eq!(changes(cx), 0);
}

#[gpui_kit::test]
fn alt_n_creates_and_alt_shift_arrows_move_the_active_workspace(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    hold(cx, window, ALT);
    press(cx, window, &view, "alt-n");
    assert_eq!(shape(cx, &view), (2, 3));
    press(cx, window, &view, "alt-shift-left");
    assert_eq!(shape(cx, &view), (1, 3));
    press(cx, window, &view, "alt-shift-right");
    assert_eq!(shape(cx, &view), (2, 3));
    hold(cx, window, Modifiers::default());
    assert_eq!(shape(cx, &view), (2, 3));
}

#[gpui_kit::test]
fn the_tray_and_workspace_keys_stay_out_of_dialogs_and_the_palette(cx: &mut TestAppContext) {
    for opener in ["f7", "ctrl-k"] {
        let (window, view) = rig(cx);
        hold(cx, window, ALT);
        press(cx, window, &view, "alt-2");
        press(cx, window, &view, opener);
        hold(cx, window, ALT);
        wait(cx, 1000);
        assert!(!tray(cx, &view));
        press(cx, window, &view, "alt-n");
        press(cx, window, &view, "alt-2");
        hold(cx, window, Modifiers::default());
        assert_eq!(shape(cx, &view), (0, 2));
    }
}

#[gpui_kit::test]
fn the_tray_and_workspace_keys_stay_out_of_the_viewer(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    let location = fake().location("home/a.txt");
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| view.view_location(location, window, cx));
    })
    .expect("window");
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.viewer.is_some()));
    hold(cx, window, ALT);
    wait(cx, 1000);
    assert!(!tray(cx, &view));
    press(cx, window, &view, "alt-n");
    press(cx, window, &view, "alt-2");
    hold(cx, window, Modifiers::default());
    assert_eq!(shape(cx, &view), (0, 2));
}
