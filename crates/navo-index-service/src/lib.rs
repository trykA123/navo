#![deny(unsafe_code)]
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "the service command and its tests report to the terminal"
)]

#[cfg(any(windows, test))]
mod answer;
#[cfg(any(windows, test))]
mod batch;
#[cfg(any(windows, test))]
mod bind_retry;
#[cfg(any(windows, test))]
mod build_permit;
#[cfg(any(windows, test))]
pub mod change_lock;
#[cfg_attr(
    not(any(windows, test)),
    allow(
        dead_code,
        reason = "the service configuration is used on Windows and in tests"
    )
)]
mod config;
#[cfg(windows)]
#[path = "directory/windows.rs"]
mod directory;
#[cfg(any(windows, test))]
mod directory_policy;
#[cfg(windows)]
#[path = "elevation/windows.rs"]
mod elevation;
mod elevation_error;
#[cfg(any(windows, test))]
mod elevation_result;

pub use elevation_error::{Elevated, ElevationError};
#[cfg(any(windows, test))]
mod heartbeat;
#[cfg(any(windows, test))]
mod install_check;
#[cfg(any(windows, test))]
mod install_plan;
#[cfg(windows)]
#[path = "installation/windows.rs"]
mod installation;
#[cfg(any(windows, test))]
mod lifecycle;
#[cfg(any(windows, test))]
mod memory;
#[cfg(windows)]
#[path = "ownership/windows.rs"]
mod ownership;
#[cfg(any(windows, test))]
mod policy;
#[cfg(any(windows, test))]
mod rebuild;
#[cfg(any(windows, test))]
mod removal;
#[cfg(windows)]
mod runtime;
#[cfg(windows)]
#[path = "scm/windows.rs"]
mod scm;
#[cfg(any(windows, test))]
mod search;
#[cfg(any(windows, test))]
mod service_log;
#[cfg(windows)]
#[path = "session/windows.rs"]
mod session;
#[cfg(windows)]
#[path = "storage/windows.rs"]
mod storage;
#[cfg(any(windows, test))]
mod totals;
#[cfg(any(windows, test))]
mod upgrade;
#[cfg(windows)]
#[path = "volumes/windows.rs"]
mod volumes;
#[cfg(windows)]
mod worker;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Run,
    Install,
    Remove,
}

impl Action {
    pub fn parse(value: &std::ffi::OsStr) -> Result<Self, String> {
        match value.to_str() {
            Some("run") => Ok(Self::Run),
            Some("install") => Ok(Self::Install),
            Some("remove") => Ok(Self::Remove),
            _ => Err("--index-service needs run, install or remove".into()),
        }
    }
}

pub fn run(action: Action) -> std::process::ExitCode {
    match execute(action) {
        Ok(outcome) => std::process::ExitCode::from(success_code(outcome)),
        Err(error) => {
            eprintln!("navo: index service: {error}");
            std::process::ExitCode::from(failure_code(&error))
        }
    }
}

#[cfg(any(windows, test))]
use elevation_result::{failure_code, success_code};

#[cfg(not(any(windows, test)))]
fn failure_code(_: &std::io::Error) -> u8 {
    1
}

#[cfg(not(any(windows, test)))]
fn success_code(_: Elevated) -> u8 {
    0
}

/// The most time the elevated remove takes; the app's uninstall callback waits longer.
pub const REMOVE_DEADLINE: std::time::Duration = std::time::Duration::from_secs(20);

#[cfg(windows)]
pub fn request_elevation(action: Action) -> Result<Elevated, ElevationError> {
    elevation::launch(action)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceQuery {
    pub running: Option<bool>,
    /// What a running service answers; `None` when it is not running or did not answer.
    pub version: Option<navo_index::pipe::ServiceVersion>,
    pub legacy_present: bool,
}

#[cfg(windows)]
pub fn query() -> std::io::Result<ServiceQuery> {
    let running = scm::query_running(install_plan::NAVO)?;
    Ok(ServiceQuery {
        running,
        version: (running == Some(true)).then(running_version).flatten(),
        legacy_present: scm::query_running(install_plan::LEGACY_PRODUCT)?.is_some(),
    })
}

#[cfg(windows)]
fn running_version() -> Option<navo_index::pipe::ServiceVersion> {
    const WAIT: std::time::Duration = std::time::Duration::from_secs(2);
    match navo_index::pipe::Client::version(std::time::Instant::now() + WAIT) {
        Ok(version) => Some(version),
        Err(error) => {
            tracing::debug!(%error, "index service: no version answer");
            None
        }
    }
}

pub use config::Drive as ServiceDrive;

#[cfg(windows)]
pub fn installed_volume_roots(search: &navo_store::Search) -> std::io::Result<Option<Vec<String>>> {
    service_volume_roots(scm::query_enabled, volumes::discover, search)
}

#[cfg(any(windows, test))]
fn service_volume_roots(
    enabled: impl FnOnce(install_plan::Product) -> std::io::Result<Option<bool>>,
    discover: impl FnOnce() -> std::io::Result<Vec<ServiceDrive>>,
    search: &navo_store::Search,
) -> std::io::Result<Option<Vec<String>>> {
    if enabled(install_plan::NAVO)? != Some(true) {
        return Ok(None);
    }
    selected_roots(search, &discover()?).map(Some)
}

pub fn selected_roots(
    search: &navo_store::Search,
    available: &[ServiceDrive],
) -> std::io::Result<Vec<String>> {
    Ok(config::select_volumes(search, available)?
        .into_iter()
        .map(|drive| format!("{}:\\", drive.letter))
        .collect())
}

#[cfg(windows)]
pub fn upgrade_notice_due(marker: Option<std::path::PathBuf>) -> bool {
    upgrade::UpgradeNotice::new(marker).is_due(query)
}

#[cfg(windows)]
pub fn dismiss_upgrade_notice(marker: Option<std::path::PathBuf>) -> std::io::Result<()> {
    upgrade::UpgradeNotice::new(marker).dismiss()
}

#[cfg(windows)]
fn execute(action: Action) -> std::io::Result<Elevated> {
    match action {
        Action::Run => scm::dispatch().map(|()| Elevated::Done),
        Action::Install | Action::Remove if !elevation::is_admin() => {
            request_elevation(action).map_err(std::io::Error::other)
        }
        Action::Install => {
            let _change = change_lock::global()?;
            installation::install().map(|()| Elevated::Done)
        }
        Action::Remove => {
            let _change = change_lock::global()?;
            installation::remove()
        }
    }
}

#[cfg(not(windows))]
fn execute(_: Action) -> std::io::Result<Elevated> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "requires Windows",
    ))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod install_replace_tests;
#[cfg(test)]
mod install_rollback_tests;
#[cfg(test)]
mod persistence_tests;
#[cfg(test)]
mod reliability_tests;
#[cfg(test)]
mod timing_tests;
#[cfg(test)]
mod upgrade_tests;
