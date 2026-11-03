use std::io;
use std::path::Path;

use navo_index::pipe::ServiceVersion;
use navo_update::InstallKind;

use super::ServiceQuery;

/// Keeps the revision whose update prompt was declined or failed, under the data folder.
pub(super) const DECLINED: &str = "index-service-declined";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Step {
    Nothing,
    Elevate,
    Outdated,
}

/// A service that cannot say its revision predates `VersionV1`, so it is older.
pub(crate) fn is_older(version: &ServiceVersion, own: u32) -> bool {
    match version {
        ServiceVersion::Older => true,
        ServiceVersion::Known { revision, .. } => *revision < own,
    }
}

/// D22f: an installed Navo asks once per revision to update an older running service.
pub(super) fn service_step(
    kind: InstallKind,
    query: &ServiceQuery,
    own: u32,
    declined: Option<u32>,
) -> Step {
    let is_running_older = query.running == Some(true)
        && query
            .version
            .as_ref()
            .is_some_and(|version| is_older(version, own));
    match kind {
        _ if !is_running_older => Step::Nothing,
        InstallKind::Portable | InstallKind::Unpackaged => Step::Nothing,
        InstallKind::Installed if declined == Some(own) => Step::Outdated,
        InstallKind::Installed => Step::Elevate,
    }
}

/// The prompt counts as declined unless it succeeded and the service no longer answers older.
pub(super) fn is_updated(prompt_ok: bool, after: Option<&ServiceVersion>, own: u32) -> bool {
    prompt_ok && !after.is_some_and(|version| is_older(version, own))
}

pub(super) fn read_declined(marker: &Path) -> Option<u32> {
    std::fs::read_to_string(marker).ok()?.trim().parse().ok()
}

pub(super) fn write_declined(marker: &Path, own: u32) -> io::Result<()> {
    navo_store::write_atomic(marker, own.to_string().as_bytes())
}

#[cfg(windows)]
pub(super) use worker::start;

#[cfg(windows)]
mod worker {
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use navo_index::pipe::SERVICE_REVISION;
    use navo_update::InstallKind;

    use super::{Step, is_updated, read_declined, service_step, write_declined};

    const AFTER_FIRST_WINDOW: Duration = Duration::from_secs(5);
    const SETTLE: Duration = Duration::from_secs(5);

    /// Runs the step on its own thread once the first window had time to show.
    pub(in super::super) fn start(marker: PathBuf) {
        let spawned = std::thread::Builder::new()
            .name("index-service-step".into())
            .spawn(move || {
                std::thread::sleep(AFTER_FIRST_WINDOW);
                run(&marker);
            });
        if let Err(error) = spawned {
            tracing::warn!(%error, "index service step: no worker thread");
        }
    }

    fn run(marker: &Path) {
        let query = match super::super::query() {
            Ok(query) => query,
            Err(error) => {
                tracing::debug!(%error, "index service step: no service query");
                return;
            }
        };
        let declined = read_declined(marker);
        let step = service_step(install_kind(), &query, SERVICE_REVISION, declined);
        if step != Step::Elevate {
            tracing::debug!(?step, version = ?query.version, "index service step");
            return;
        }
        tracing::info!(version = ?query.version, own = SERVICE_REVISION, "index service is older than Navo; asking to update it");
        let prompt = super::super::request_elevation(super::super::Action::Install);
        std::thread::sleep(SETTLE);
        let after = super::super::query().ok().and_then(|query| query.version);
        if is_updated(prompt.is_ok(), after.as_ref(), SERVICE_REVISION) {
            tracing::info!(version = ?after, "index service updated");
            return;
        }
        let reason = prompt.err().map(|error| error.to_string());
        tracing::warn!(?reason, version = ?after, "index service update declined or failed; not asking again for this revision");
        if let Err(error) = write_declined(marker, SERVICE_REVISION) {
            tracing::warn!(%error, "index service step: cannot remember the declined revision");
        }
    }

    fn install_kind() -> InstallKind {
        match velopack::UpdateManager::new(velopack::sources::NoneSource {}, None, None) {
            Ok(manager) => crate::updater::feed::install_kind(&manager),
            Err(_) => InstallKind::Unpackaged,
        }
    }
}

#[cfg(test)]
#[path = "version_step_tests.rs"]
mod tests;
