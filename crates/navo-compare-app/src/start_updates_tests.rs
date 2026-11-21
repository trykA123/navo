//! Tests of the Start screen's update line: its four views, its buttons and its focus handles.

use std::sync::Arc;

use gpui_kit::base::test_support::texts;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, Entity, Modifiers, TestAppContext, VisualTestContext};
use navo_compare_profile::appearance::{UiPrefs, UpdateCheck};
use navo_compare_profile::update_state::UpdateState;
use sidewise_l10n::t;
use sidewise_update::check::CheckError;
use sidewise_update::version::{Version, release_page};

use super::*;
use crate::shell_prefs::{prefs, save_prefs};
use crate::test_window;
use crate::updates::{Fetch, RUNNING, install_for_test};

/// A fixed clock.
fn clock() -> i64 {
    1_000_000
}

/// A stable release one patch above the running build.
fn newer() -> Version {
    let running = Version::parse(RUNNING).expect("running version");
    Version::new(running.major, running.minor, running.patch + 1)
}

/// Installs the global for `choice` and `state`, with a fetch that fails at once, and opens the line.
fn open_line(
    choice: UpdateCheck,
    state: UpdateState,
    cx: &mut TestAppContext,
) -> (AnyWindowHandle, Entity<UpdateLine>) {
    cx.update(|cx| {
        save_prefs(
            UiPrefs {
                update_check: choice,
                ..UiPrefs::default()
            },
            cx,
        )
        .expect("save");
        let fetch: Fetch = Arc::new(|| Err(CheckError::Unsupported));
        install_for_test(state, fetch, clock, cx);
    });
    let opened = test_window::open(cx, |_, cx| UpdateLine::new(cx));
    cx.update_window(opened.0, |_, window, cx| window.render_frame(cx))
        .expect("window");
    opened
}

/// The four views with the plain texts and the button ids each must draw (a button's label is its id's message).
fn views() -> Vec<(UpdateCheck, UpdateState, Vec<String>, Vec<&'static str>)> {
    let prompt = UpdateState {
        launches: 3,
        ..UpdateState::default()
    };
    let found = UpdateState {
        latest: Some(newer().to_string()),
        ..UpdateState::default()
    };
    vec![
        (
            UpdateCheck::Off,
            UpdateState::default(),
            vec![t!("update-line-off")],
            vec!["update-turn-on"],
        ),
        (
            UpdateCheck::Ask,
            prompt,
            vec![t!("update-prompt-title"), t!("update-prompt-body")],
            vec!["update-turn-on", "update-not-now"],
        ),
        (
            UpdateCheck::On,
            UpdateState::default(),
            vec![t!("update-line-on")],
            vec!["update-turn-off"],
        ),
        (
            UpdateCheck::On,
            found,
            vec![t!("update-available", version = newer().to_string())],
            vec!["update-release-page", "update-turn-off"],
        ),
    ]
}

/// Every visible text in the window.
fn shown(cx: &mut TestAppContext, window: AnyWindowHandle) -> Vec<String> {
    cx.update_window(window, |_, w, _| {
        texts(w).iter().map(|t| t.text().to_string()).collect()
    })
    .expect("window")
}

/// Clicks the center of the element `id`, then settles and redraws.
fn click(cx: &mut TestAppContext, window: AnyWindowHandle, id: &'static str) {
    let center = cx
        .update_window(window, |_, w, _| w.find(id).bounds().center())
        .expect("window");
    VisualTestContext::from_window(window, cx).simulate_click(center, Modifiers::none());
    cx.run_until_parked();
    cx.update_window(window, |_, w, cx| w.render_frame(cx))
        .expect("window");
}

#[gpui_kit::test]
fn each_view_draws_its_texts_and_buttons(cx: &mut TestAppContext) {
    for (choice, state, want_texts, want_ids) in views() {
        let want_labels: Vec<(&str, String)> = want_ids.iter().map(|id| (*id, t!(id))).collect();
        let (window, line) = open_line(choice, state, cx);
        let all = shown(cx, window);
        for want in &want_texts {
            assert!(all.contains(want), "{choice:?}: {want:?} in {all:?}");
        }
        for (id, label) in &want_labels {
            let got = cx
                .update_window(window, |_, w, _| w.find(*id).label().map(str::to_owned))
                .expect("window");
            assert_eq!(got.as_ref(), Some(label), "{choice:?}: button {id}");
        }
        let drawn = cx
            .update_window(window, |_, w, _| {
                [
                    "update-turn-on",
                    "update-not-now",
                    "update-turn-off",
                    "update-release-page",
                ]
                .into_iter()
                .filter(|id| w.try_find(*id).is_some())
                .count()
            })
            .expect("window");
        assert_eq!(drawn, want_ids.len(), "{choice:?}: buttons drawn");
        let handles = cx.update(|cx| line.read(cx).handles(cx));
        assert_eq!(
            handles.iter().flatten().count(),
            want_ids.len(),
            "{choice:?}"
        );
    }
}

#[gpui_kit::test]
fn turn_on_sets_the_choice_on(cx: &mut TestAppContext) {
    let (window, _) = open_line(UpdateCheck::Off, UpdateState::default(), cx);
    click(cx, window, "update-turn-on");
    cx.update(|cx| assert_eq!(prefs(cx).update_check, UpdateCheck::On));
    let off = cx
        .update_window(window, |_, w, _| w.try_find("update-turn-off").is_some())
        .expect("window");
    assert!(off, "the On line offers Turn off");
}

#[gpui_kit::test]
fn not_now_sets_off_and_the_card_is_gone(cx: &mut TestAppContext) {
    let state = UpdateState {
        launches: 3,
        ..UpdateState::default()
    };
    let (window, _) = open_line(UpdateCheck::Ask, state, cx);
    click(cx, window, "update-not-now");
    cx.update(|cx| assert_eq!(prefs(cx).update_check, UpdateCheck::Off));
    let all = shown(cx, window);
    assert!(!all.contains(&t!("update-prompt-title")), "{all:?}");
    assert!(all.contains(&t!("update-line-off")), "{all:?}");
}

#[gpui_kit::test]
fn release_page_opens_the_found_version(cx: &mut TestAppContext) {
    let state = UpdateState {
        latest: Some(newer().to_string()),
        ..UpdateState::default()
    };
    let (window, _) = open_line(UpdateCheck::On, state, cx);
    click(cx, window, "update-release-page");
    assert_eq!(cx.opened_url(), Some(release_page(&newer())));
}

#[gpui_kit::test]
fn without_the_global_nothing_is_drawn(cx: &mut TestAppContext) {
    let (window, line) = test_window::open(cx, |_, cx| UpdateLine::new(cx));
    cx.update_window(window, |_, w, cx| w.render_frame(cx))
        .expect("window");
    let drawn = cx
        .update_window(window, |_, w, _| w.try_find("update-line").is_some())
        .expect("window");
    assert!(!drawn);
    let handles = cx.update(|cx| line.read(cx).handles(cx));
    assert!(handles.iter().all(Option::is_none));
}

/// Opens a Start screen with no rows and returns its window.
fn start_window(cx: &mut TestAppContext) -> AnyWindowHandle {
    let cwd = std::path::PathBuf::from(".");
    crate::start_tests::screen(cx, navo_compare_profile::recent::StartRows::default(), &cwd).0
}

#[gpui_kit::test]
fn the_start_screen_shows_the_off_line_under_the_list(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let fetch: Fetch = Arc::new(|| Err(CheckError::Unsupported));
        install_for_test(UpdateState::default(), fetch, clock, cx);
    });
    let window = start_window(cx);
    assert!(shown(cx, window).contains(&t!("update-line-off")));
}

#[gpui_kit::test]
fn the_start_screen_without_the_global_shows_no_line(cx: &mut TestAppContext) {
    let window = start_window(cx);
    let drawn = cx
        .update_window(window, |_, w, _| w.try_find("update-line").is_some())
        .expect("window");
    assert!(!drawn);
}
