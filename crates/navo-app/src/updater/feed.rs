//! Update feed over the GitHub releases of Navo, through Velopack (D89).

#![expect(dead_code, reason = "the runner wiring follows in IX8o")]

use std::io;
use std::sync::mpsc;
use std::sync::{Mutex, PoisonError};
use std::thread;

use navo_update::{FeedError, Found, InstallKind, Release, UpdateFeed, Version, parse_version};
use velopack::sources::GithubSource;
use velopack::{Error, NetworkError, UpdateCheck, UpdateInfo, UpdateManager, UpdateOptions};

const REPO: &str = "https://github.com/trykA123/navo";

/// Portable when Velopack says so, else installed. Needs a manager, so never `Unpackaged`.
pub(crate) fn install_kind(manager: &UpdateManager) -> InstallKind {
    if manager.get_is_portable() {
        InstallKind::Portable
    } else {
        InstallKind::Installed
    }
}

/// Checks and downloads from GitHub releases. The last found update is kept for the download.
pub(crate) struct VelopackFeed {
    manager: UpdateManager,
    kind: InstallKind,
    found: Mutex<Option<UpdateInfo>>,
}

impl VelopackFeed {
    /// `Err(Unpackaged)` when this copy has no Velopack install around it.
    pub(crate) fn detect() -> Result<Self, InstallKind> {
        let options = UpdateOptions {
            AllowVersionDowngrade: false,
            ..Default::default()
        };
        let source = GithubSource::new(REPO, None, true);
        match UpdateManager::new(source, Some(options), None) {
            Ok(manager) => Ok(Self {
                kind: install_kind(&manager),
                manager,
                found: Mutex::new(None),
            }),
            Err(Error::NotInstalled(_)) => Err(InstallKind::Unpackaged),
            Err(error) => {
                tracing::warn!(%error, "update: Velopack could not be opened");
                Err(InstallKind::Unpackaged)
            }
        }
    }

    /// The update to install, or `None` when the feed has nothing newer.
    fn check_info(&self) -> Result<Option<UpdateInfo>, FeedError> {
        match self.manager.check_for_updates().map_err(|e| classify(&e))? {
            UpdateCheck::UpdateAvailable(info) if !info.IsDowngrade => Ok(Some(*info)),
            UpdateCheck::UpdateAvailable(_) | UpdateCheck::NoUpdateAvailable => Ok(None),
            UpdateCheck::RemoteIsEmpty => Err(FeedError::BadFeed),
        }
    }

    fn remember(&self, info: Option<UpdateInfo>) {
        *self.found.lock().unwrap_or_else(PoisonError::into_inner) = info;
    }

    /// The kept update when it is `release`, else a fresh check that must offer the same version.
    fn info_for(&self, release: &Release) -> Result<UpdateInfo, FeedError> {
        let kept = self
            .found
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let info = match kept {
            Some(info) => info,
            None => self.check_info()?.ok_or_else(changed)?,
        };
        if version_of(&info)? == release.version {
            Ok(info)
        } else {
            Err(changed())
        }
    }
}

fn changed() -> FeedError {
    FeedError::Other("the feed no longer offers this release".to_owned())
}

fn version_of(info: &UpdateInfo) -> Result<Version, FeedError> {
    parse_version(&info.TargetFullRelease.Version)
}

impl UpdateFeed for VelopackFeed {
    fn kind(&self) -> InstallKind {
        self.kind
    }

    fn current(&self) -> Version {
        self.manager.get_current_version()
    }

    fn staged(&self) -> Option<Version> {
        let asset = self.manager.get_update_pending_restart()?;
        parse_version(&asset.Version).ok()
    }

    fn check(&self) -> Result<Found, FeedError> {
        let Some(info) = self.check_info()? else {
            self.remember(None);
            return Ok(Found::Nothing);
        };
        let release = Release {
            version: version_of(&info)?,
            size: info.TargetFullRelease.Size,
        };
        self.remember(Some(info));
        Ok(Found::Release(release))
    }

    fn download(&self, release: &Release, progress: &dyn Fn(u8)) -> Result<(), FeedError> {
        let info = self.info_for(release)?;
        let manager = self.manager.clone();
        let (sender, receiver) = mpsc::channel();
        let worker = thread::spawn(move || manager.download_updates(&info, Some(sender)));
        // The channel closes when the download returns and drops its sender.
        for percent in receiver {
            progress(percent.clamp(0, 100) as u8);
        }
        match worker.join() {
            Ok(result) => result.map_err(|error| classify(&error)),
            Err(_) => Err(FeedError::Other(
                "the download stopped unexpectedly".to_owned(),
            )),
        }
    }
}

/// Maps a Velopack error to what the update machine acts on.
pub(crate) fn classify(error: &Error) -> FeedError {
    match error {
        Error::Network(network) => from_status(http_status(network)),
        Error::Io(error) if lock_taken(error) => FeedError::Busy,
        Error::Io(_) => FeedError::Offline,
        Error::Json(_) | Error::Semver(_) | Error::InvalidPackage(_) => FeedError::BadFeed,
        Error::ChecksumInvalid(..) | Error::SizeInvalid(..) => FeedError::Corrupt,
        other => FeedError::Other(other.to_string()),
    }
}

fn from_status(status: Option<u16>) -> FeedError {
    match status {
        Some(403 | 429) => FeedError::RateLimited,
        _ => FeedError::Offline,
    }
}

/// The HTTP status of a failed request; `None` for any other transport error.
fn http_status(error: &NetworkError) -> Option<u16> {
    let NetworkError::Http(inner) = error else {
        return None;
    };
    parse_status(&inner.to_string())
}

/// ureq words a status failure as `http status: 403`.
fn parse_status(text: &str) -> Option<u16> {
    text.strip_prefix("http status: ")?.parse().ok()
}

/// Velopack's non-blocking lock on the packages folder failed because another holder has it.
fn lock_taken(error: &io::Error) -> bool {
    if cfg!(windows) {
        // ERROR_SHARING_VIOLATION and ERROR_LOCK_VIOLATION.
        matches!(error.raw_os_error(), Some(32 | 33))
    } else {
        error
            .to_string()
            .to_lowercase()
            .starts_with("failed to lock file")
    }
}

#[cfg(test)]
#[path = "feed_tests.rs"]
mod tests;
