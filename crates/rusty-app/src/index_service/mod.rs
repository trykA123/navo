#![deny(unsafe_code)]

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
#[cfg(windows)]
mod runtime;
#[cfg(windows)]
#[path = "scm/windows.rs"]
mod scm;
#[cfg(windows)]
#[path = "session/windows.rs"]
mod session;
#[cfg(windows)]
#[path = "storage/windows.rs"]
mod storage;
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
            eprintln!("ferrite: index service: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(windows)]
pub(crate) fn request_elevation(action: Action) -> Result<(), elevation_result::ElevationError> {
    elevation::launch(action)
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
mod reliability_tests;
