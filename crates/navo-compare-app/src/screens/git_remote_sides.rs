//! Remote Git sides in the compare run and the diff: the run fetches each side once into the
//! process's one remote cache; diffs and menus only read what an earlier run fetched.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use navo_compare_profile::book::profile_text;
use navo_compare_profile::paths::open_profile;
use sidewise_core::Location;
use sidewise_engine::model::{Reason, SideEntry, SideSource};
use sidewise_git::location::GitAt;
use sidewise_git_remote::at::{self, PROVIDER, RemoteAt};
use sidewise_git_remote::{Cache, RemoteProblem, fetch_side};
use sidewise_host::compare::handle::{CompareTarget, OpenFailure};

use super::git_sides::{GitSides, failure_of as git_failure, file_side};
use super::host_parts::profile_choice;

/// How a run fetches a remote side; tests swap the network fetch for a local one.
type Fetch = Arc<dyn Fn(&RemoteAt) -> Result<GitAt, RemoteProblem> + Send + Sync>;

/// The folder inside the profile's cache folder that holds the remote repositories.
const CACHE_FOLDER: &str = "git-remote";

/// True when `location` names a remote Git side.
pub(crate) fn is_remote(location: &Location) -> bool {
    location.provider().as_str() == PROVIDER
}

/// The one remote cache of this process, made on first use under the profile's cache folder.
/// `None` when the profile cannot open: a remote side then fails as `Broken`, never panics.
pub(crate) fn remote_cache() -> Option<&'static Cache> {
    static CACHE: OnceLock<Option<Cache>> = OnceLock::new();
    CACHE.get_or_init(|| cache_root().map(Cache::new)).as_ref()
}

/// The cache folder of the profile chosen at start. A profile error is logged.
fn cache_root() -> Option<PathBuf> {
    let Some(explicit) = profile_choice() else {
        return unchosen_root();
    };
    match open_profile(explicit.as_deref()) {
        Ok(profile) => Some(profile.cache_dir().join(CACHE_FOLDER)),
        Err(error) => {
            tracing::warn!(error = %profile_text(&error), "remote Git sides are off this run");
            None
        }
    }
}

/// With no profile chosen (only tests), a folder of this process under the system temp folder.
#[cfg(test)]
fn unchosen_root() -> Option<PathBuf> {
    Some(std::env::temp_dir().join(format!("sidewise-git-remote-test-{}", std::process::id())))
}

/// With no profile chosen the app has not started: there is no place for a cache.
#[cfg(not(test))]
fn unchosen_root() -> Option<PathBuf> {
    None
}

/// A fetch problem as an opener failure: its compare reason and its l10n text.
fn failure_of(problem: RemoteProblem) -> OpenFailure {
    OpenFailure {
        reason: problem.reason(),
        text_id: Some(problem.fluent_id()),
    }
}

/// The network fetch of the app: WinHTTP into the remote cache (`Unsupported` off Windows).
fn network_fetch() -> Fetch {
    Arc::new(|at| match remote_cache() {
        Some(cache) => fetch_side(cache, at),
        None => Err(RemoteProblem::Broken),
    })
}

/// The target of a run for a `git-remote` location: the opener fetches the side, then opens it
/// as a local Git side. It runs on the session thread, so the UI never waits on the network.
pub(crate) fn remote_target(location: &Location, sides: Arc<GitSides>) -> CompareTarget {
    target_with(location, sides, network_fetch())
}

/// `remote_target` with the fetch given.
fn target_with(location: &Location, sides: Arc<GitSides>, fetch: Fetch) -> CompareTarget {
    let location = location.clone();
    CompareTarget::Side(Box::new(move || {
        let at = at::parse(&location).map_err(|_| OpenFailure::from(Reason::Unsupported))?;
        let stored = fetch(&at).map_err(failure_of)?;
        let side: Arc<dyn SideSource> = sides.fresh(&stored).map_err(git_failure)?;
        Ok(side)
    }))
}

/// A diff side for a `git-remote` file: read from what a run fetched, never from the network.
/// Nothing fetched is `NotFound`; an unparsable location is `Unsupported`.
pub(crate) fn remote_side(
    location: &Location,
    sides: &GitSides,
) -> Result<(Arc<dyn SideSource>, SideEntry), Reason> {
    remote_side_with(location, sides, &|at| remote_cache()?.local_at(at))
}

/// `remote_side` with the lookup of an earlier fetch given.
fn remote_side_with(
    location: &Location,
    sides: &GitSides,
    local: &dyn Fn(&RemoteAt) -> Option<GitAt>,
) -> Result<(Arc<dyn SideSource>, SideEntry), Reason> {
    let at = at::parse(location).map_err(|_| Reason::Unsupported)?;
    let stored = local(&at).ok_or(Reason::NotFound)?;
    file_side(sides, &stored)
}

#[cfg(test)]
#[path = "git_remote_sides_tests.rs"]
mod tests;
