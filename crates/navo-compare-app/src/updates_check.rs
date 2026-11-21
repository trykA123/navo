//! Runs the daily update check off the UI thread and applies what the user chooses.

use std::time::Duration;

use navo_compare_profile::appearance::{UiPrefs, UpdateCheck};
use navo_compare_profile::update_state;
use sidewise_kit::prelude::*;
use sidewise_update::check::{Outcome, due, run_check};
use sidewise_update::version::release_page;

use crate::layers::toast;
use crate::shell_prefs::{prefs, save_prefs};
use crate::updates::{RUNNING, Updates};

/// The wait after launch before the first check, so start-up is not slowed.
pub(crate) const START_DELAY: Duration = Duration::from_secs(5);

/// When the choice is On, waits `START_DELAY` and then checks if a check is due.
pub(crate) fn check_at_start(cx: &mut App) {
    if prefs(cx).update_check != UpdateCheck::On {
        return;
    }
    cx.spawn(async move |cx| {
        cx.background_executor().timer(START_DELAY).await;
        cx.update(check_if_due);
    })
    .detach();
}

/// The updates global for writing, `None` when it is not installed.
fn updates_mut(cx: &mut App) -> Option<&mut Updates> {
    cx.has_global::<Updates>()
        .then(|| cx.global_mut::<Updates>())
}

/// Starts one check on the background executor when the choice is On, none runs and one is due.
pub(crate) fn check_if_due(cx: &mut App) {
    if prefs(cx).update_check != UpdateCheck::On {
        return;
    }
    let Some(updates) = updates_mut(cx) else {
        return;
    };
    if updates.checking || !due(updates.state.last_check, (updates.now)()) {
        return;
    }
    updates.checking = true;
    let fetch = updates.fetch.clone();
    let job = cx
        .background_executor()
        .spawn(async move { run_check(RUNNING, || fetch()) });
    cx.spawn(async move |cx| {
        let outcome = job.await;
        cx.update(|cx| finish(outcome, cx));
    })
    .detach();
}

/// Records the outcome in the state (a failure keeps the version found before), saves it and redraws.
fn finish(outcome: Outcome, cx: &mut App) {
    let Some(updates) = updates_mut(cx) else {
        return;
    };
    updates.checking = false;
    updates.state.last_check = Some((updates.now)());
    updates.state.last_result = Some(outcome.short());
    match outcome {
        Outcome::Newer(v) => {
            updates.state.latest = Some(v.to_string());
            updates.found = Some(v);
        }
        Outcome::Current => {
            updates.state.latest = None;
            updates.found = None;
        }
        Outcome::Failed(_) => {}
    }
    save_state(cx);
    cx.refresh_windows();
}

/// Saves the update state to the profile, if any; an error is dropped.
fn save_state(cx: &App) {
    if let Some(updates) = cx.try_global::<Updates>()
        && let Some(profile) = &updates.profile
    {
        let _ = update_state::save(profile, &updates.state);
    }
}

/// Saves the user's choice (a save error becomes a toast), redraws, and checks at once when it is On.
pub(crate) fn set_choice(choice: UpdateCheck, cx: &mut App) {
    if let Err(message) = save_prefs(
        UiPrefs {
            update_check: choice,
            ..prefs(cx)
        },
        cx,
    ) {
        toast::notify(message, cx);
    }
    cx.refresh_windows();
    if choice == UpdateCheck::On {
        check_if_due(cx);
    }
}

/// Hides the notice for the version found: remembers it as dismissed, saves and redraws.
pub(crate) fn dismiss(cx: &mut App) {
    let Some(updates) = updates_mut(cx) else {
        return;
    };
    let Some(found) = updates.found.clone() else {
        return;
    };
    updates.state.dismissed = Some(found.to_string());
    save_state(cx);
    cx.refresh_windows();
}

/// Opens the release page of the version found in the browser.
pub(crate) fn open_release_page(cx: &mut App) {
    let found = cx.try_global::<Updates>().and_then(|u| u.found.clone());
    if let Some(v) = found {
        cx.open_url(&release_page(&v));
    }
}

#[cfg(test)]
#[path = "updates_check_tests.rs"]
mod tests;
