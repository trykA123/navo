//! Tests of the update check driver: when it runs, what it records and what the user's choices do.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use gpui_kit::TestAppContext;
use navo_compare_profile::appearance::{UiPrefs, UpdateCheck};
use navo_compare_profile::update_state::UpdateState;
use sidewise_update::check::CheckError;
use sidewise_update::version::{Version, release_page};

use navo_compare_profile::paths::open_profile;
use navo_compare_profile::update_state;
use sidewise_update::check::Outcome;

use super::*;
use crate::shell_prefs::{prefs, save_prefs};
use crate::updates::{Fetch, install_for_test, install_updates, notice};

/// A fixed clock.
fn clock() -> i64 {
    1_000_000
}

/// A stable release one patch above the running build: offered on either channel.
fn newer() -> Version {
    let running = Version::parse(RUNNING).expect("running version");
    Version::new(running.major, running.minor, running.patch + 1)
}

/// A feed holding one release, `newer()`.
fn feed() -> Vec<u8> {
    format!(
        r#"[{{"tag_name":"v{}","prerelease":false,"draft":false}}]"#,
        newer()
    )
    .into_bytes()
}

/// A fake fetch that counts its calls and returns `result` at once.
fn counting(result: Result<Vec<u8>, CheckError>) -> (Fetch, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let fetch: Fetch = Arc::new(move || {
        seen.fetch_add(1, Ordering::SeqCst);
        result.clone()
    });
    (fetch, calls)
}

/// Sets the choice, then installs the global with `fetch` and a fresh state.
fn setup(choice: UpdateCheck, fetch: Fetch, cx: &mut TestAppContext) {
    cx.update(|cx| {
        save_prefs(
            UiPrefs {
                update_check: choice,
                ..UiPrefs::default()
            },
            cx,
        )
        .expect("save");
        install_for_test(UpdateState::default(), fetch, clock, cx);
    });
}

/// Runs the start check and lets the clock and the executor finish it.
fn start(cx: &mut TestAppContext) {
    cx.update(check_at_start);
    cx.executor().advance_clock(START_DELAY);
    cx.run_until_parked();
}

#[gpui_kit::test]
fn ask_and_off_never_fetch(cx: &mut TestAppContext) {
    for choice in [UpdateCheck::Ask, UpdateCheck::Off] {
        let (fetch, calls) = counting(Ok(feed()));
        setup(choice, fetch, cx);
        start(cx);
        assert_eq!(calls.load(Ordering::SeqCst), 0, "{choice:?}");
    }
}

#[gpui_kit::test]
fn on_checks_once_and_records_the_newer_version(cx: &mut TestAppContext) {
    let (fetch, calls) = counting(Ok(feed()));
    setup(UpdateCheck::On, fetch, cx);
    start(cx);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    cx.update(|cx| {
        let u = cx.global::<Updates>();
        assert_eq!(u.found, Some(newer()));
        assert_eq!(u.state.last_result, Some(format!("newer {}", newer())));
        assert!(!u.checking);
        check_if_due(cx);
    });
    cx.run_until_parked();
    assert_eq!(calls.load(Ordering::SeqCst), 1, "not due again at once");
}

#[gpui_kit::test]
fn a_failed_fetch_keeps_found_and_records_the_error(cx: &mut TestAppContext) {
    let (fetch, calls) = counting(Err(CheckError::Network(12029)));
    setup(UpdateCheck::On, fetch, cx);
    cx.update(|cx| cx.global_mut::<Updates>().found = Some(newer()));
    start(cx);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    cx.update(|cx| {
        let u = cx.global::<Updates>();
        assert_eq!(u.found, Some(newer()));
        assert_eq!(u.state.last_result.as_deref(), Some("network 12029"));
    });
}

#[gpui_kit::test]
fn choosing_on_checks_at_once_and_saves_the_choice(cx: &mut TestAppContext) {
    let (fetch, calls) = counting(Ok(feed()));
    setup(UpdateCheck::Ask, fetch, cx);
    cx.update(|cx| set_choice(UpdateCheck::On, cx));
    cx.run_until_parked();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    cx.update(|cx| assert_eq!(prefs(cx).update_check, UpdateCheck::On));
}

#[gpui_kit::test]
fn dismiss_hides_the_notice_and_the_release_page_opens(cx: &mut TestAppContext) {
    let (fetch, _) = counting(Ok(feed()));
    setup(UpdateCheck::On, fetch, cx);
    start(cx);
    cx.update(|cx| {
        assert_eq!(notice(cx), Some(newer()));
        open_release_page(cx);
        dismiss(cx);
        assert_eq!(notice(cx), None);
    });
    assert_eq!(cx.opened_url(), Some(release_page(&newer())));
}

#[gpui_kit::test]
fn dismiss_without_a_found_version_does_nothing(cx: &mut TestAppContext) {
    let (fetch, _) = counting(Ok(feed()));
    setup(UpdateCheck::On, fetch, cx);
    cx.update(|cx| {
        dismiss(cx);
        assert_eq!(cx.global::<Updates>().state.dismissed, None);
        assert_eq!(notice(cx), None);
    });
}

/// A temporary profile folder, removed on drop.
struct Temp(std::path::PathBuf);

impl Drop for Temp {
    /// Removes the folder and everything in it.
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[gpui_kit::test]
fn finish_and_dismiss_write_update_check_toml_and_a_reload_shows_them(cx: &mut TestAppContext) {
    let temp = Temp(std::env::temp_dir().join(format!("sidewise-u10a-{}", std::process::id())));
    std::fs::create_dir_all(&temp.0).expect("create temp folder");
    cx.update(|cx| {
        save_prefs(
            UiPrefs {
                update_check: UpdateCheck::On,
                ..UiPrefs::default()
            },
            cx,
        )
        .expect("save");
        install_updates(Some(&temp.0), cx);
        finish(Outcome::Newer(newer()), cx);
    });
    let profile = open_profile(Some(&temp.0)).expect("open profile");
    assert!(
        profile.update_state_file().exists(),
        "update-check.toml written"
    );
    let saved = update_state::load(&profile);
    assert_eq!(saved.latest, Some(newer().to_string()));
    assert_eq!(saved.last_result, Some(format!("newer {}", newer())));
    assert!(saved.last_check.is_some());
    assert_eq!(saved.dismissed, None);
    cx.update(dismiss);
    let saved = update_state::load(&profile);
    assert_eq!(saved.dismissed, Some(newer().to_string()));
    cx.update(|cx| {
        install_updates(Some(&temp.0), cx);
        assert_eq!(notice(cx), None, "the reload keeps the notice hidden");
        assert_eq!(cx.global::<Updates>().found, Some(newer()));
    });
}
