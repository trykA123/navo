//! Tests of the update check global: the launch count, the view per choice and the notice.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::TestAppContext;
use navo_compare_profile::appearance::{UiPrefs, UpdateCheck};
use navo_compare_profile::paths::open_profile;
use navo_compare_profile::update_state::{self, UpdateState};
use sidewise_update::check::CheckError;
use sidewise_update::version::Version;

use super::*;
use crate::shell_prefs::{install_prefs, save_prefs};

/// A temporary profile folder, removed on drop.
struct Temp(PathBuf);

impl Temp {
    /// Makes an empty folder named after the test.
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("sidewise-u8-{}-{name}", std::process::id()));
        fs::create_dir_all(&path).expect("create temp folder");
        Self(path)
    }
}

impl Drop for Temp {
    /// Removes the folder and everything in it.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A fetch that always fails: these tests never reach the network.
fn no_fetch() -> Fetch {
    Arc::new(|| Err(CheckError::Unsupported))
}

/// A fixed clock.
fn clock() -> i64 {
    1_000
}

/// Installs prefs with `choice`, then the updates global, from the folder.
fn install(temp: &Temp, choice: UpdateCheck, cx: &mut TestAppContext) {
    cx.update(|cx| {
        install_prefs(Some(&temp.0), cx).expect("prefs");
        save_prefs(
            UiPrefs {
                update_check: choice,
                ..prefs(cx)
            },
            cx,
        )
        .expect("save");
        install_updates(Some(&temp.0), cx);
    });
}

/// The saved launch count of the global.
fn launches(cx: &mut TestAppContext) -> u32 {
    cx.update(|cx| cx.global::<Updates>().state.launches)
}

/// With Ask, each install counts a launch up to 3, and Start turns to the prompt at 3.
#[gpui_kit::test]
fn ask_counts_launches_to_three(cx: &mut TestAppContext) {
    let temp = Temp::new("ask");
    for want in [1, 2, 3, 3] {
        install(&temp, UpdateCheck::Ask, cx);
        assert_eq!(launches(cx), want);
        let shown = cx.update(|cx| view(cx));
        let expect = if want == 3 {
            UpdateView::Prompt
        } else {
            UpdateView::Off
        };
        assert_eq!(shown, Some(expect));
    }
    let profile = open_profile(Some(&temp.0)).expect("open");
    assert_eq!(update_state::load(&profile).launches, 3, "saved");
}

/// With On, a launch is not counted.
#[gpui_kit::test]
fn on_never_counts(cx: &mut TestAppContext) {
    let temp = Temp::new("on");
    install(&temp, UpdateCheck::On, cx);
    assert_eq!(launches(cx), 0);
}

/// A saved version at or below this build gives no `found`; a newer one does.
#[gpui_kit::test]
fn found_needs_a_newer_version(cx: &mut TestAppContext) {
    let state = |latest: &str| UpdateState {
        latest: Some(latest.into()),
        ..UpdateState::default()
    };
    cx.update(|cx| install_for_test(state("0.1.0-alpha.1"), no_fetch(), clock, cx));
    assert_eq!(cx.update(|cx| cx.global::<Updates>().found.clone()), None);
    cx.update(|cx| install_for_test(state("0.1.0-alpha.2"), no_fetch(), clock, cx));
    assert_eq!(cx.update(|cx| cx.global::<Updates>().found.clone()), None);
    cx.update(|cx| install_for_test(state("0.1.0-alpha.99"), no_fetch(), clock, cx));
    let want = Version::parse("0.1.0-alpha.99").ok();
    assert_eq!(cx.update(|cx| cx.global::<Updates>().found.clone()), want);
}

/// `view` and `notice` follow the choice; a hidden version hides the notice only.
#[gpui_kit::test]
fn view_and_notice_follow_the_choice(cx: &mut TestAppContext) {
    let v = Version::parse("0.1.0-alpha.99").expect("version");
    let state = UpdateState {
        latest: Some(v.to_string()),
        launches: PROMPT_AFTER,
        ..UpdateState::default()
    };
    cx.update(|cx| assert_eq!(view(cx), None));
    let set = |choice, state: UpdateState, cx: &mut TestAppContext| {
        cx.update(|cx| {
            save_prefs(
                UiPrefs {
                    update_check: choice,
                    ..UiPrefs::default()
                },
                cx,
            )
            .expect("save");
            install_for_test(state, no_fetch(), clock, cx);
        });
    };
    set(UpdateCheck::Off, state.clone(), cx);
    cx.update(|cx| {
        assert_eq!(view(cx), Some(UpdateView::Off));
        assert_eq!(notice(cx), None);
    });
    set(UpdateCheck::Ask, state.clone(), cx);
    cx.update(|cx| {
        assert_eq!(view(cx), Some(UpdateView::Prompt));
        assert_eq!(notice(cx), None);
    });
    set(UpdateCheck::On, state.clone(), cx);
    cx.update(|cx| {
        assert_eq!(view(cx), Some(UpdateView::Found(v.clone())));
        assert_eq!(notice(cx), Some(v.clone()));
    });
    let hidden = UpdateState {
        dismissed: Some(v.to_string()),
        ..state.clone()
    };
    set(UpdateCheck::On, hidden, cx);
    cx.update(|cx| {
        assert_eq!(view(cx), Some(UpdateView::Found(v.clone())));
        assert_eq!(notice(cx), None);
    });
    set(UpdateCheck::On, UpdateState::default(), cx);
    cx.update(|cx| assert_eq!(view(cx), Some(UpdateView::On)));
}
