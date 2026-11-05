use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::ERROR_SERVICE_DOES_NOT_EXIST;
use windows_sys::Win32::Storage::FileSystem::{DELETE, MOVEFILE_DELAY_UNTIL_REBOOT, MoveFileExW};
use windows_sys::Win32::System::Services::{
    DeleteService, OpenServiceW, SC_MANAGER_CONNECT, SERVICE_QUERY_STATUS, SERVICE_STOP,
};
use windows_sys::Win32::UI::Shell::{FOLDERID_ProgramData, FOLDERID_ProgramFiles};

use super::super::directory::{Policy, ProtectedDirectory, known_folder};
use super::super::install_plan::{CopyPlan, LEGACY_PRODUCT, NAVO, Product, clear_directory};
use super::super::removal::{
    self, Cleanup, Entry, Files, Folders, Limits, REMOVE_DEADLINE, StopLimits,
};
use super::super::scm::{checked, open_manager, service_handle, service_name_of};
use super::super::upgrade::remove_both;

const FILE_TRIES: u32 = 5;
const FILE_DELAY: Duration = Duration::from_millis(200);

pub(super) fn deadline() -> Instant {
    Instant::now() + REMOVE_DEADLINE
}

/// Removes both services and their files within `REMOVE_DEADLINE`; returns what is left.
pub(super) fn remove_all(deadline: Instant) -> io::Result<Cleanup> {
    let mut cleanup = Cleanup::default();
    remove_both([NAVO, LEGACY_PRODUCT], |product| {
        removal::run(
            || remove_service(product, deadline),
            || {
                cleanup.extend(remove_files(product, deadline)?);
                Ok(())
            },
        )
    })?;
    Ok(cleanup)
}

pub(super) fn report(cleanup: &Cleanup) {
    for path in &cleanup.scheduled {
        tracing::warn!(path = %path.display(), "index service file scheduled for deletion at restart");
        eprintln!(
            "navo: index service: {} is deleted at the next restart",
            path.display()
        );
    }
    for path in &cleanup.unknown {
        tracing::warn!(path = %path.display(), "unknown file kept in the index service folder");
        eprintln!("navo: index service: kept unknown {}", path.display());
    }
}

pub(super) fn remove_service(product: Product, deadline: Instant) -> io::Result<()> {
    let manager = open_manager(SC_MANAGER_CONNECT)?;
    let name = service_name_of(product);
    // SAFETY: manager and terminated name are live for the call.
    let raw = unsafe {
        OpenServiceW(
            manager.0,
            name.as_ptr(),
            DELETE | SERVICE_STOP | SERVICE_QUERY_STATUS,
        )
    };
    let service = match service_handle(raw) {
        Ok(service) => service,
        Err(error) if error.raw_os_error() == Some(ERROR_SERVICE_DOES_NOT_EXIST as i32) => {
            return Err(io::ErrorKind::NotFound.into());
        }
        Err(error) => return Err(error),
    };
    super::scm_stop::stop(&service, StopLimits::within(deadline))?;
    // SAFETY: service has DELETE access and its process has exited.
    checked(unsafe { DeleteService(service.0) })
}

pub(super) fn remove_files(product: Product, deadline: Instant) -> io::Result<Cleanup> {
    let binary = remove_binary(product, deadline);
    let data = remove_data(product);
    let cleanup = binary?;
    data?;
    Ok(cleanup)
}

fn remove_binary(product: Product, deadline: Instant) -> io::Result<Cleanup> {
    let plan = CopyPlan::of(product, &known_folder(&FOLDERID_ProgramFiles)?, 0);
    let parent = plan
        .directory
        .parent()
        .ok_or_else(|| io::Error::other("missing install parent"))?;
    let Some(guard) = open_existing(&[parent, &plan.directory], Policy::Executable)? else {
        return Ok(Cleanup::default());
    };
    let limits = Limits {
        tries: FILE_TRIES,
        delay: FILE_DELAY,
        deadline,
        process: std::process::id(),
    };
    let folders = Folders {
        parent,
        index: &plan.directory,
    };
    removal::clean(
        &mut WindowsFiles { guard },
        folders,
        product.removable,
        &limits,
    )
}

fn remove_data(product: Product) -> io::Result<()> {
    let [parent, index] = product.data_directories(&known_folder(&FOLDERID_ProgramData)?);
    if let Some(mut guard) = open_existing(&[&parent, &index], Policy::Data)? {
        clear_directory(&index)?;
        guard.remove_last(&index)?;
        return guard.remove_last(&parent);
    }
    match open_existing(&[&parent], Policy::Data)? {
        Some(mut guard) => guard.remove_last(&parent),
        None => Ok(()),
    }
}

fn open_existing(paths: &[&Path], policy: Policy) -> io::Result<Option<ProtectedDirectory>> {
    match ProtectedDirectory::open(paths, policy, false) {
        Ok(guard) => Ok(Some(guard)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// The guard holds the parent and then the index folder; `clean` removes them in that reverse order.
struct WindowsFiles {
    guard: ProtectedDirectory,
}

impl Files for WindowsFiles {
    fn list(&mut self, folder: &Path) -> io::Result<Vec<Entry>> {
        std::fs::read_dir(folder)?
            .map(|entry| {
                let entry = entry?;
                Ok(Entry {
                    is_file: entry.file_type()?.is_file(),
                    path: entry.path(),
                })
            })
            .collect()
    }

    fn delete(&mut self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }

    fn rename(&mut self, from: &Path, to: &Path) -> io::Result<()> {
        move_file(from, Some(to), 0)
    }

    fn schedule(&mut self, path: &Path) -> io::Result<()> {
        move_file(path, None, MOVEFILE_DELAY_UNTIL_REBOOT)
    }

    fn remove_folder(&mut self, path: &Path) -> io::Result<()> {
        self.guard.release_last();
        std::fs::remove_dir(path)
    }
}

/// Zero flags refuse an existing target; a NULL target with DELAY_UNTIL_REBOOT schedules a delete.
pub(super) fn move_file(from: &Path, to: Option<&Path>, flags: u32) -> io::Result<()> {
    let from = terminated(from);
    let to = to.map(terminated);
    let target = to.as_ref().map_or(ptr::null(), |to| to.as_ptr());
    // SAFETY: both strings are NUL-terminated and outlive the call; the target may be NULL.
    checked(unsafe { MoveFileExW(from.as_ptr(), target, flags) })
}

fn terminated(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}
