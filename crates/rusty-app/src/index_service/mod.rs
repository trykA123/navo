#![deny(unsafe_code)]

#[cfg(any(windows, test))]
mod answer;
#[cfg(any(windows, test))]
mod bind_retry;
#[cfg(any(windows, test))]
mod config;
#[cfg(windows)]
#[path = "directory/windows.rs"]
mod directory;
#[cfg(any(windows, test))]
mod directory_policy;
#[cfg(windows)]
#[path = "elevation/windows.rs"]
mod elevation;
#[cfg(any(windows, test))]
mod elevation_result;
#[cfg(any(windows, test))]
mod memory;
#[cfg(any(windows, test))]
mod totals;
#[cfg(any(windows, test))]
pub(crate) use elevation_result::ElevationError;
#[cfg(any(windows, test))]
mod install_plan;
#[cfg(windows)]
#[path = "installation/windows.rs"]
mod installation;
#[cfg(any(windows, test))]
mod lifecycle;
#[cfg(windows)]
#[path = "ownership/windows.rs"]
mod ownership;
#[cfg(any(windows, test))]
mod policy;
#[cfg(any(windows, test))]
mod removal;
#[cfg(windows)]
mod runtime;
#[cfg(windows)]
#[path = "scm/windows.rs"]
mod scm;
#[cfg(any(windows, test))]
mod search;
#[cfg(windows)]
#[path = "session/windows.rs"]
mod session;
#[cfg(windows)]
#[path = "storage/windows.rs"]
mod storage;
#[cfg(any(windows, test))]
mod upgrade;
#[cfg(windows)]
#[path = "volumes/windows.rs"]
mod volumes;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Run,
    Install,
    Remove,
}

impl Action {
    pub(crate) fn parse(value: &std::ffi::OsStr) -> Result<Self, String> {
        match value.to_str() {
            Some("run") => Ok(Self::Run),
            Some("install") => Ok(Self::Install),
            Some("remove") => Ok(Self::Remove),
            _ => Err("--index-service needs run, install or remove".into()),
        }
    }
}

pub(crate) fn run(action: Action) -> std::process::ExitCode {
    match execute(action) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("navo: index service: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(windows)]
pub(crate) fn request_elevation(action: Action) -> Result<(), ElevationError> {
    elevation::launch(action)
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ServiceQuery {
    pub running: Option<bool>,
    pub legacy_present: bool,
}

#[cfg(windows)]
pub(crate) fn query() -> std::io::Result<ServiceQuery> {
    Ok(ServiceQuery {
        running: scm::query_running(install_plan::NAVO)?,
        legacy_present: scm::query_running(install_plan::FERRITE)?.is_some(),
    })
}

#[cfg(any(windows, test))]
pub(crate) use config::Drive as ServiceDrive;

#[cfg(windows)]
pub(crate) fn installed_volume_roots(
    search: &rusty_store::Search,
) -> std::io::Result<Option<Vec<String>>> {
    service_volume_roots(scm::query_enabled, volumes::discover, search)
}

#[cfg(any(windows, test))]
fn service_volume_roots(
    enabled: impl FnOnce(install_plan::Product) -> std::io::Result<Option<bool>>,
    discover: impl FnOnce() -> std::io::Result<Vec<ServiceDrive>>,
    search: &rusty_store::Search,
) -> std::io::Result<Option<Vec<String>>> {
    if enabled(install_plan::NAVO)? != Some(true) {
        return Ok(None);
    }
    selected_roots(search, &discover()?).map(Some)
}

#[cfg(any(windows, test))]
pub(crate) fn selected_roots(
    search: &rusty_store::Search,
    available: &[ServiceDrive],
) -> std::io::Result<Vec<String>> {
    Ok(config::select_volumes(search, available)?
        .into_iter()
        .map(|drive| format!("{}:\\", drive.letter))
        .collect())
}

#[cfg(windows)]
pub(crate) fn upgrade_notice_due(marker: Option<std::path::PathBuf>) -> bool {
    upgrade::UpgradeNotice::new(marker).is_due(query)
}

#[cfg(windows)]
pub(crate) fn dismiss_upgrade_notice(marker: Option<std::path::PathBuf>) -> std::io::Result<()> {
    upgrade::UpgradeNotice::new(marker).dismiss()
}

#[cfg(windows)]
fn execute(action: Action) -> std::io::Result<()> {
    match action {
        Action::Run => scm::dispatch(),
        Action::Install | Action::Remove if !elevation::is_admin() => {
            request_elevation(action).map_err(std::io::Error::other)
        }
        Action::Install => installation::install(),
        Action::Remove => installation::remove(),
    }
}

#[cfg(not(windows))]
fn execute(_: Action) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "requires Windows",
    ))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod install_rollback_tests;
#[cfg(test)]
mod reliability_tests;
#[cfg(test)]
mod timing_tests;
#[cfg(test)]
mod upgrade_tests;
