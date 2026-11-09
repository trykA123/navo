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
    processes::any_under(root)
}

/// Never applies off Windows.
#[cfg(not(windows))]
fn others_running(_root: Option<&Path>) -> io::Result<bool> {
    Ok(true)
}

/// An image we cannot read blocks the apply when it runs in this session and its name is this
/// image's name or starts with `navo`; the index service runs in session 0 as `navo-index.exe`.
#[cfg(any(windows, test))]
fn unreadable_counts(name: &str, own: &str, same_session: bool) -> bool {
    let name = name.to_lowercase();
    same_session && (name == own || name.starts_with("navo"))
}

/// This process and its children (the PDF helper) never block a restart.
#[cfg(any(windows, test))]
fn is_other(pid: u32, parent: u32, own: u32) -> bool {
    pid != 0 && pid != own && parent != own
}

#[cfg(windows)]
mod processes {
    use std::ffi::OsString;
    use std::io;
    use std::os::windows::ffi::OsStringExt;
    use std::path::{Path, PathBuf};

    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_NO_MORE_FILES, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };

    const PATH_UNITS: usize = 32_768;

    struct Owned(HANDLE);

    impl Drop for Owned {
        fn drop(&mut self) {
            // SAFETY: this guard owns a live handle that nothing else closes.
            unsafe { CloseHandle(self.0) };
        }
    }

    pub(super) fn any_under(root: &Path) -> io::Result<bool> {
        let mut roots = vec![normalized(root)];
        if let Ok(canonical) = std::fs::canonicalize(root) {
            roots.push(normalized(&canonical));
        }
        let own_name = std::env::current_exe()?
            .file_name()
            .map(|name| name.to_string_lossy().to_lowercase())
            .ok_or_else(|| io::Error::other("own image has no name"))?;
        let own = std::process::id();
        let own_image = Own {
            name: own_name,
            session: session(own),
        };
        let size = u32::try_from(size_of::<PROCESSENTRY32W>()).map_err(io::Error::other)?;
        // SAFETY: no pointers are passed; the result is checked before use.
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot == INVALID_HANDLE_VALUE || snapshot.is_null() {
            return Err(io::Error::last_os_error());
        }
        let snapshot = Owned(snapshot);
        let mut entry = PROCESSENTRY32W {
            dwSize: size,
            ..PROCESSENTRY32W::default()
        };
        let mut path = vec![0u16; PATH_UNITS];
        // SAFETY: the snapshot handle is live and `entry` is writable with dwSize set.
        if unsafe { Process32FirstW(snapshot.0, &raw mut entry) } == 0 {
            return Err(io::Error::last_os_error());
        }
        loop {
            if super::is_other(entry.th32ProcessID, entry.th32ParentProcessID, own)
                && belongs(&entry, &mut path, &roots, &own_image)
            {
                return Ok(true);
            }
            entry.dwSize = size;
            // SAFETY: as above; the snapshot stays open until `snapshot` drops.
            if unsafe { Process32NextW(snapshot.0, &raw mut entry) } == 0 {
                return end_of_list(io::Error::last_os_error());
            }
        }
    }

    /// The end of the snapshot is "nobody found"; any other error is a failure.
    fn end_of_list(error: io::Error) -> io::Result<bool> {
        if error.raw_os_error() == i32::try_from(ERROR_NO_MORE_FILES).ok() {
            Ok(false)
        } else {
            Err(error)
        }
    }

    /// This process's image name and Windows session.
    struct Own {
        name: String,
        session: Option<u32>,
    }

    /// A process whose image cannot be read counts when its name looks like a Navo image.
    fn belongs(entry: &PROCESSENTRY32W, path: &mut [u16], roots: &[PathBuf], own: &Own) -> bool {
        match image(entry.th32ProcessID, path) {
            Some(image) => {
                let image = normalized(&image);
                roots.iter().any(|root| image.starts_with(root))
            }
            None => {
                let name = entry
                    .szExeFile
                    .split(|unit| *unit == 0)
                    .next()
                    .unwrap_or_default();
                let same_session =
                    own.session.is_some() && session(entry.th32ProcessID) == own.session;
                super::unreadable_counts(&String::from_utf16_lossy(name), &own.name, same_session)
            }
        }
    }

    fn session(pid: u32) -> Option<u32> {
        let mut session = 0u32;
        // SAFETY: `session` is a writable u32 for the duration of the call.
        let success = unsafe { ProcessIdToSessionId(pid, &raw mut session) };
        (success != 0).then_some(session)
    }

    fn image(pid: u32, path: &mut [u16]) -> Option<PathBuf> {
        // SAFETY: requests only limited query access for a process ID, with no inherited handle.
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            return None;
        }
        let process = Owned(handle);
        let mut length = u32::try_from(path.len()).ok()?;
        // SAFETY: the process handle is live and `path` has `length` writable UTF-16 units.
        let success = unsafe {
            QueryFullProcessImageNameW(
                process.0,
                PROCESS_NAME_WIN32,
                path.as_mut_ptr(),
                &raw mut length,
            )
        };
        if success == 0 {
            return None;
        }
        let length = usize::try_from(length).ok()?;
        Some(PathBuf::from(OsString::from_wide(path.get(..length)?)))
    }

    /// Lowercase, without the `\\?\` prefix, so the two path forms compare by component.
    fn normalized(path: &Path) -> PathBuf {
        let text = path.to_string_lossy().to_lowercase();
        let text = match text.strip_prefix(r"\\?\unc\") {
            Some(rest) => format!(r"\\{rest}"),
            None => text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned(),
        };
        PathBuf::from(text)
    }
}

#[cfg(test)]
#[path = "apply_tests.rs"]
mod tests;
