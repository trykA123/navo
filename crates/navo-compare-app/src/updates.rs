//! The update check as an app global: the saved state, what the user chose, and the newer version, if any.

use std::path::Path;
use std::sync::Arc;

use gpui_kit::Global;
use navo_compare_profile::appearance::UpdateCheck;
use navo_compare_profile::paths::{Profile, open_profile};
use navo_compare_profile::update_state::{self, UpdateState};
use sidewise_kit::prelude::*;
use sidewise_update::check::CheckError;
use sidewise_update::fetch::fetch_feed;
use sidewise_update::version::Version;

use crate::shell_prefs::prefs;

/// Starts after this many launches with the choice still Ask turn the Start line into the prompt card.
pub(crate) const PROMPT_AFTER: u32 = 3;
/// The version of this build.
pub(crate) const RUNNING: &str = env!("CARGO_PKG_VERSION");

/// The blocking feed download; tests inject a fake, the app uses `fetch_feed`.
pub(crate) type Fetch = Arc<dyn Fn() -> Result<Vec<u8>, CheckError> + Send + Sync>;

/// The update check's state: saved machine state, the profile it saves to and the newer version, if known.
pub(crate) struct Updates {
    pub(crate) state: UpdateState,
    pub(crate) profile: Option<Profile>,
    pub(crate) found: Option<Version>,
    pub(crate) checking: bool,
    pub(crate) fetch: Fetch,
    pub(crate) now: fn() -> i64,
}
impl Global for Updates {}

/// What the Start screen shows for the update check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UpdateView {
    Off,
    Prompt,
    On,
    Found(Version),
}

/// The current time in Unix seconds; 0 when the clock is before 1970.
fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .unwrap_or(0)
}

/// `state.latest` as a version, kept only when it is newer than this build by semver precedence.
fn found_of(state: &UpdateState) -> Option<Version> {
    let running = Version::parse(RUNNING).ok()?;
    let latest = Version::parse(state.latest.as_deref()?).ok()?;
    latest.cmp_precedence(&running).is_gt().then_some(latest)
}

/// Installs the global; never fails. Without a profile it keeps the default state and saves nothing.
/// While the choice is Ask it counts this launch (up to `PROMPT_AFTER`); a save error is dropped.
pub(crate) fn install_updates(explicit: Option<&Path>, cx: &mut App) {
    let profile = open_profile(explicit).ok();
    let mut state = profile.as_ref().map(update_state::load).unwrap_or_default();
    if prefs(cx).update_check == UpdateCheck::Ask && state.launches < PROMPT_AFTER {
        state.launches += 1;
        if let Some(profile) = &profile {
            let _ = update_state::save(profile, &state);
        }
    }
    let found = found_of(&state);
    cx.set_global(Updates {
        state,
        profile,
        found,
        checking: false,
        fetch: Arc::new(fetch_feed),
        now: now_unix,
    });
}

/// What Start shows: `None` without the global; by choice, Off, the prompt, On or the newer version.
pub(crate) fn view(cx: &App) -> Option<UpdateView> {
    let updates = cx.try_global::<Updates>()?;
    Some(match prefs(cx).update_check {
        UpdateCheck::Off => UpdateView::Off,
        UpdateCheck::Ask if updates.state.launches >= PROMPT_AFTER => UpdateView::Prompt,
        UpdateCheck::Ask => UpdateView::Off,
        UpdateCheck::On => match &updates.found {
            Some(v) => UpdateView::Found(v.clone()),
            None => UpdateView::On,
        },
    })
}

/// The newer version to announce: choice On, one is known and the user has not hidden it.
pub(crate) fn notice(cx: &App) -> Option<Version> {
    let updates = cx.try_global::<Updates>()?;
    if prefs(cx).update_check != UpdateCheck::On {
        return None;
    }
    let found = updates.found.clone()?;
    (Some(found.to_string()) != updates.state.dismissed).then_some(found)
}

/// Installs the global without a profile, with an injected fetch and clock.
#[cfg(test)]
pub(crate) fn install_for_test(state: UpdateState, fetch: Fetch, now: fn() -> i64, cx: &mut App) {
    let found = found_of(&state);
    cx.set_global(Updates {
        state,
        profile: None,
        found,
        checking: false,
        fetch,
        now,
    });
}

#[cfg(test)]
#[path = "updates_tests.rs"]
mod tests;
