//! Applies a staged update at the start of a window launch (D22d).

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use navo_ui::window_view::UpdateFailure;
use navo_update::{InstallKind, Version, parse_version};
use velopack::{UpdateManager, VelopackAsset, sources::NoneSource};

use super::feed::install_kind;
use crate::cli::Command;
use crate::{installer, logging};

/// A downloaded package and the version this copy runs.
pub(crate) struct Staged<A> {
    pub(crate) current: Version,
    pub(crate) version: Version,
    pub(crate) asset: A,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Apply<A> {
    Now(A),
    Wait(Reason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reason {
    NotWindow,
    NotInstalled(InstallKind),
    NothingStaged,
    NotNewer,
    OthersRunning,
    QueryFailed,
    RelativeArgs,
}

/// `Now` only for an installed copy opening a window, with a staged newer package and no
/// other process of this install. Processes are queried last, and only when it matters.
pub(crate) fn apply_decision<A>(
    kind: InstallKind,
    staged: Option<Staged<A>>,
    others_running: impl FnOnce() -> io::Result<bool>,
    command: &Command,
) -> Apply<A> {
    if !matches!(command, Command::Open(_)) {
        return Apply::Wait(Reason::NotWindow);
    }
    if kind != InstallKind::Installed {
        return Apply::Wait(Reason::NotInstalled(kind));
    }
    let Some(staged) = staged else {
        return Apply::Wait(Reason::NothingStaged);
    };
    if staged.version <= staged.current {
        return Apply::Wait(Reason::NotNewer);
    }
    match others_running() {
        Ok(false) => Apply::Now(staged.asset),
        Ok(true) => Apply::Wait(Reason::OthersRunning),
        Err(_) => Apply::Wait(Reason::QueryFailed),
    }
}

/// Applies a staged package and restarts with `args`; returns when the package waits.
/// On success Velopack exits this process, so call it before anything else starts.
pub(crate) fn at_start(command: &Command, args: &[OsString]) {
    if !matches!(command, Command::Open(_)) {
        return;
    }
    if has_relative_value(args) {
        tracing::debug!(reason = ?Reason::RelativeArgs, "update: nothing applied at start");
        return;
    }
    let Ok(manager) = UpdateManager::new(NoneSource {}, None, None) else {
        return;
    };
    let kind = install_kind(&manager);
    // A start right after a Velopack restart never applies again, so a failed apply cannot loop.
    let staged = (!installer::restarted())
        .then(|| staged(&manager))
        .flatten();
    let decision = apply_decision(kind, staged, || others_running(root().as_deref()), command);
    match decision {
        Apply::Now(asset) => {
            tracing::info!(version = %asset.Version, "update: applying the staged package");
            logging::flush();
            if let Err(error) = manager.apply_updates_and_restart_with_args(&asset, args) {
                tracing::warn!(%error, "update: the staged package could not be applied");
            }
        }
        Apply::Wait(reason) => tracing::debug!(?reason, "update: nothing applied at start"),
    }
}

pub(crate) fn restart() -> Result<(), UpdateFailure> {
    let command =
        crate::cli::parse(std::env::args_os().skip(1)).map_err(|_| UpdateFailure::Apply)?;
    let args = restart_args(&command).map_err(|_| UpdateFailure::Apply)?;
    let manager =
        UpdateManager::new(NoneSource {}, None, None).map_err(|_| UpdateFailure::Apply)?;
    let decision = apply_decision(
        install_kind(&manager),
        staged(&manager),
        || others_running(root().as_deref()),
        &command,
    );
    restart_with(decision, crate::files::stop_pdf, |asset| {
        logging::flush();
        manager
            .wait_exit_then_apply_updates(&asset, false, true, &args)
            .map_err(|error| {
                tracing::warn!(%error, "update: restart handoff failed");
                UpdateFailure::Apply
            })
    })
}

/// `stop` ends the child helpers, which run this exe, before the handoff; it runs only for `Now`.
fn restart_with<A>(
    decision: Apply<A>,
    stop: impl FnOnce(),
    apply: impl FnOnce(A) -> Result<(), UpdateFailure>,
) -> Result<(), UpdateFailure> {
    match decision {
        Apply::Now(asset) => {
            stop();
            apply(asset)
        }
        Apply::Wait(Reason::OthersRunning) => Err(UpdateFailure::OthersRunning),
        Apply::Wait(_) => Err(UpdateFailure::Apply),
    }
}

fn restart_args(command: &Command) -> io::Result<Vec<OsString>> {
    let Command::Open(start) = command else {
        return Err(io::Error::other("restart requires a window"));
    };
    let mut args = Vec::new();
    if let Some(profile) = &start.profile {
        args.push(OsString::from("--profile"));
        args.push(std::path::absolute(profile)?.into_os_string());
    }
    if !start.retention {
        args.push(OsString::from("--no-retention"));
    }
    Ok(args)
}

/// Velopack restarts Navo in its own working directory, so a relative value would change
/// meaning; such a start waits for a launch with plain flags or absolute paths.
fn has_relative_value(args: &[OsString]) -> bool {
    args.iter().any(|arg| {
        let text = arg.to_string_lossy();
        !text.starts_with('-') && !Path::new(arg).is_absolute()
    })
}

fn staged(manager: &UpdateManager) -> Option<Staged<VelopackAsset>> {
    let asset = manager.get_update_pending_restart()?;
    let version = parse_version(&asset.Version).ok()?;
    Some(Staged {
        current: manager.get_current_version(),
        version,
        asset,
    })
}

#[cfg(windows)]
fn root() -> Option<PathBuf> {
    use velopack::locator::{LocationContext, auto_locate_app_manifest};
    auto_locate_app_manifest(LocationContext::FromCurrentExe)
        .ok()
        .map(|locator| locator.get_root_dir())
}

#[cfg(not(windows))]
fn root() -> Option<PathBuf> {
    None
}

/// Whether another process runs an image under the install `root`. Any doubt is `Err`.
#[cfg(windows)]
fn others_running(root: Option<&Path>) -> io::Result<bool> {
    let root = root.ok_or_else(|| io::Error::other("install folder unknown"))?;
    navo_diag::processes::any_under(root)
}

/// Never applies off Windows.
#[cfg(not(windows))]
fn others_running(_root: Option<&Path>) -> io::Result<bool> {
    Ok(true)
}

#[cfg(test)]
#[path = "apply_tests.rs"]
mod tests;
