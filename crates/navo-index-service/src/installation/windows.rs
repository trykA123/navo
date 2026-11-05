#![allow(unsafe_code, reason = "SCM installation requires Win32 FFI")]

use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr;
use std::time::Duration;

use windows_sys::Win32::Foundation::{ERROR_SERVICE_ALREADY_RUNNING, ERROR_SERVICE_EXISTS};
use windows_sys::Win32::Storage::FileSystem::MOVEFILE_DELAY_UNTIL_REBOOT;
use windows_sys::Win32::System::Services::{
    ChangeServiceConfig2W, ChangeServiceConfigW, CreateServiceW, DeleteService, OpenServiceW,
    QueryServiceStatus, SC_ACTION, SC_ACTION_NONE, SC_ACTION_RESTART, SC_MANAGER_CONNECT,
    SC_MANAGER_CREATE_SERVICE, SERVICE_ALL_ACCESS, SERVICE_AUTO_START,
    SERVICE_CONFIG_DELAYED_AUTO_START_INFO, SERVICE_CONFIG_DESCRIPTION,
    SERVICE_CONFIG_FAILURE_ACTIONS, SERVICE_CONFIG_FAILURE_ACTIONS_FLAG,
    SERVICE_DELAYED_AUTO_START_INFO, SERVICE_DESCRIPTIONW, SERVICE_ERROR_NORMAL,
    SERVICE_FAILURE_ACTIONS_FLAG, SERVICE_FAILURE_ACTIONSW, SERVICE_STATUS, SERVICE_STOPPED,
    SERVICE_WIN32_OWN_PROCESS, StartServiceW,
};
use windows_sys::Win32::UI::Shell::{FOLDERID_ProgramData, FOLDERID_ProgramFiles};

use super::Elevated;
use super::directory::{Policy, ProtectedDirectory, known_folder};
use super::elevation::wide;
use super::install_check::{delete_legacy, service_command, service_source};
use super::install_plan::{
    CopyPlan, FreshInstall, LEGACY_PRODUCT, SettingsBackup, Step, data_directories, retry,
    run_install,
};
use super::policy::{FAILURE_DELAYS, RESET_SECONDS, RESTART_CLEAN_STOPS};
use super::removal::StopLimits;
use super::scm::{ServiceHandle, checked, open_manager, service_handle, service_name};
use super::storage::Storage;
use super::upgrade::{UpgradeStep, run_upgrade};

mod cleanup;
mod configuration;
mod legacy;
mod scm_stop;
mod verify;

const ANSWER_WAIT: (Duration, Duration) = (Duration::from_secs(30), Duration::from_millis(250));
const DESCRIPTION: &str = "Keeps the file index for Navo search up to date.";
const LEGACY_TRIES: u32 = 5;
const LEGACY_DELAY: Duration = Duration::from_millis(200);

/// The installed service and its verified process, held until the install is done.
struct Installed {
    service: ServiceHandle,
    process: verify::Bound,
}

pub(super) fn install() -> io::Result<()> {
    let source = service_source(&std::env::current_exe()?)?;
    let legacy = legacy::detect()?;
    let config = match legacy::carried_config() {
        Some(config) => config,
        None => super::session::installation_config()?,
    };
    let mut installed = None;
    let leftover = run_upgrade(legacy, |step| match step {
        UpgradeStep::InstallNew => {
            install_current(&source, &config).map(|done| installed = Some(done))
        }
        UpgradeStep::AwaitNew => installed
            .as_ref()
            .ok_or_else(unverified)
            .and_then(|done| answer(&done.service, Some(&done.process))),
        UpgradeStep::RemoveLegacyService => {
            cleanup::remove_service(LEGACY_PRODUCT, cleanup::deadline())
        }
        UpgradeStep::RemoveLegacyFiles => {
            cleanup::remove_files(LEGACY_PRODUCT, cleanup::deadline())
                .map(|left| cleanup::report(&left))
        }
    })?;
    warn_leftover(leftover);
    Ok(())
}

fn warn_leftover(leftover: Option<io::Error>) {
    if let Some(warning) = leftover {
        tracing::warn!(%warning, "index service installed with old files left in place");
        eprintln!("navo: index service: warning: {warning}");
    }
}

fn unverified() -> io::Error {
    io::Error::other("the service answer was checked before its process was verified")
}

fn answer(service: &ServiceHandle, process: Option<&verify::Bound>) -> io::Result<()> {
    let process = process.ok_or_else(unverified)?;
    verify::answer(service, process, ANSWER_WAIT)
}

fn install_current(source: &Path, config: &super::config::Config) -> io::Result<Installed> {
    let plan = CopyPlan::new(&known_folder(&FOLDERID_ProgramFiles)?, std::process::id());
    let data = data_directories(&known_folder(&FOLDERID_ProgramData)?);
    let parent = plan
        .directory
        .parent()
        .ok_or_else(|| io::Error::other("missing install parent"))?;
    let directories = [
        parent.to_path_buf(),
        plan.directory.clone(),
        data[0].clone(),
        data[1].clone(),
    ];
    let mut fresh = FreshInstall::observe(&plan, &directories);
    let result = install_with((&plan, source), config, &mut fresh);
    if result.is_err() {
        let _ = fresh.remove_directories();
    }
    result
}

fn install_with(
    (plan, source): (&CopyPlan, &Path),
    config: &super::config::Config,
    fresh: &mut FreshInstall,
) -> io::Result<Installed> {
    let storage = Storage::open(true)?;
    let parent = plan
        .directory
        .parent()
        .ok_or_else(|| io::Error::other("missing install parent"))?;
    let _guard = ProtectedDirectory::open(&[parent, &plan.directory], Policy::Executable, true)?;
    let command = binary_command(plan);
    let manager = open_manager(SC_MANAGER_CREATE_SERVICE | SC_MANAGER_CONNECT)?;
    let (service, exists) = register(&manager, &command)?;
    let was_running = exists && is_running(&service)?;
    let previous = exists
        .then(|| configuration::Configuration::read(&service))
        .transpose()?;
    let (mut settings, mut started_after, mut process) = (None, None, None);
    let result = run_install(exists, was_running, |step| match step {
        Step::RecoverOld => fresh.recover_executable(plan),
        Step::Stop => {
            scm_stop::stop(&service, StopLimits::FULL).map(|()| started_after = Some(verify::now()))
        }
        Step::Copy => plan.copy_from(source),
        Step::RenameOld => plan.rename_old(),
        Step::Replace => plan.replace(),
        Step::BackupSettings => SettingsBackup::capture(&storage.directory.join("service.json"))
            .map(|backup| settings = Some(backup)),
        Step::Configure => configure(&service, &command, &storage, config),
        Step::Start => {
            started_after.get_or_insert_with(verify::now);
            start(&service)
        }
        Step::Verify => verify::installed(&service, (plan, source), &command, started_after)
            .map(|bound| process = Some(bound)),
        Step::Answer => answer(&service, process.as_ref()),
        Step::DeleteLegacy => remove_legacy(plan),
        Step::Restore => plan.restore(),
        Step::RestoreSettings => settings.as_ref().map_or(Ok(()), SettingsBackup::restore),
        Step::RemoveFresh => fresh.remove_executable(plan),
        Step::DiscardCopy => plan.discard_copy(),
        Step::RestoreConfiguration => previous
            .as_ref()
            .map_or(Ok(()), |old| old.restore(&service)),
        Step::DeleteOld => plan.delete_old(),
    });
    finish_install(result.map(warn_leftover), exists, &service)?;
    let process = process.ok_or_else(unverified)?;
    Ok(Installed { service, process })
}

fn configure(
    service: &ServiceHandle,
    command: &[u16],
    storage: &Storage,
    config: &super::config::Config,
) -> io::Result<()> {
    update(service, command)?;
    describe(service)?;
    configure_recovery(service)?;
    storage.write_config(config)
}

/// Past the commit point: deletes `Index\navo.exe` and its `.old` copies, scheduling a refusal.
fn remove_legacy(plan: &CopyPlan) -> io::Result<()> {
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(&plan.directory)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            paths.push(entry.path());
        }
    }
    delete_legacy(paths, remove_existing, |path: &Path| {
        cleanup::move_file(path, None, MOVEFILE_DELAY_UNTIL_REBOOT)
    })
}

fn remove_existing(path: &Path) -> io::Result<()> {
    retry(LEGACY_TRIES, LEGACY_DELAY, || {
        match std::fs::remove_file(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            result => result,
        }
    })
}

fn finish_install(result: io::Result<()>, exists: bool, service: &ServiceHandle) -> io::Result<()> {
    if let Err(error) = result {
        if !exists {
            // SAFETY: this call rolls back only the service just created here.
            checked(unsafe { DeleteService(service.0) }).map_err(|rollback| {
                io::Error::other(format!("{error}; service rollback: {rollback}"))
            })?;
        }
        return Err(error);
    }
    Ok(())
}

fn is_running(service: &ServiceHandle) -> io::Result<bool> {
    let mut status = SERVICE_STATUS::default();
    // SAFETY: service is live with QUERY_STATUS access; status is writable.
    checked(unsafe { QueryServiceStatus(service.0, &mut status) })?;
    Ok(status.dwCurrentState != SERVICE_STOPPED)
}

fn start(service: &ServiceHandle) -> io::Result<()> {
    // SAFETY: service is live with START access and no arguments are passed.
    match checked(unsafe { StartServiceW(service.0, 0, ptr::null()) }) {
        Err(error) if error.raw_os_error() == Some(ERROR_SERVICE_ALREADY_RUNNING as i32) => Ok(()),
        result => result,
    }
}

fn binary_command(plan: &CopyPlan) -> Vec<u16> {
    service_command(&plan.executable)
        .encode_wide()
        .chain(Some(0))
        .collect()
}

fn register(manager: &ServiceHandle, command: &[u16]) -> io::Result<(ServiceHandle, bool)> {
    let name = service_name();
    let account = wide("LocalSystem");
    // SAFETY: all handles and terminated strings remain live; optional parameters are NULL.
    let raw = unsafe {
        CreateServiceW(
            manager.0,
            name.as_ptr(),
            name.as_ptr(),
            SERVICE_ALL_ACCESS,
            SERVICE_WIN32_OWN_PROCESS,
            SERVICE_AUTO_START,
            SERVICE_ERROR_NORMAL,
            command.as_ptr(),
            ptr::null(),
            ptr::null_mut(),
            ptr::null(),
            account.as_ptr(),
            ptr::null(),
        )
    };
    match service_handle(raw) {
        Ok(service) => Ok((service, false)),
        Err(error) if error.raw_os_error() == Some(ERROR_SERVICE_EXISTS as i32) => {
            // SAFETY: manager and name are live; all installation rights are requested.
            let raw = unsafe { OpenServiceW(manager.0, name.as_ptr(), SERVICE_ALL_ACCESS) };
            Ok((service_handle(raw)?, true))
        }
        Err(error) => Err(error),
    }
}

fn update(service: &ServiceHandle, command: &[u16]) -> io::Result<()> {
    let account = wide("LocalSystem");
    // SAFETY: service and strings are live; NULL leaves optional configuration unchanged.
    checked(unsafe {
        ChangeServiceConfigW(
            service.0,
            SERVICE_WIN32_OWN_PROCESS,
            SERVICE_AUTO_START,
            SERVICE_ERROR_NORMAL,
            command.as_ptr(),
            ptr::null(),
            ptr::null_mut(),
            ptr::null(),
            account.as_ptr(),
            ptr::null(),
            ptr::null(),
        )
    })
}

fn describe(service: &ServiceHandle) -> io::Result<()> {
    let mut text = wide(DESCRIPTION);
    let info = SERVICE_DESCRIPTIONW {
        lpDescription: text.as_mut_ptr(),
    };
    // SAFETY: info points to the live NUL-terminated description; service is live.
    checked(unsafe {
        ChangeServiceConfig2W(
            service.0,
            SERVICE_CONFIG_DESCRIPTION,
            (&info as *const SERVICE_DESCRIPTIONW).cast(),
        )
    })
}

fn configure_recovery(service: &ServiceHandle) -> io::Result<()> {
    let delayed = SERVICE_DELAYED_AUTO_START_INFO {
        fDelayedAutostart: 1,
    };
    let mut actions = FAILURE_DELAYS.map(|delay| SC_ACTION {
        Type: if delay.is_some() {
            SC_ACTION_RESTART
        } else {
            SC_ACTION_NONE
        },
        Delay: delay.unwrap_or(0),
    });
    let failure = SERVICE_FAILURE_ACTIONSW {
        dwResetPeriod: RESET_SECONDS,
        cActions: actions.len() as u32,
        lpsaActions: actions.as_mut_ptr(),
        ..Default::default()
    };
    let flag = SERVICE_FAILURE_ACTIONS_FLAG {
        fFailureActionsOnNonCrashFailures: i32::from(RESTART_CLEAN_STOPS),
    };
    for (kind, info) in [
        (
            SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
            (&delayed as *const SERVICE_DELAYED_AUTO_START_INFO).cast(),
        ),
        (
            SERVICE_CONFIG_FAILURE_ACTIONS,
            (&failure as *const SERVICE_FAILURE_ACTIONSW).cast(),
        ),
        (
            SERVICE_CONFIG_FAILURE_ACTIONS_FLAG,
            (&flag as *const SERVICE_FAILURE_ACTIONS_FLAG).cast(),
        ),
    ] {
        // SAFETY: each pointer references the matching live structure; service is live.
        checked(unsafe { ChangeServiceConfig2W(service.0, kind, info) })?;
    }
    Ok(())
}

/// Removes both services and their files; prints what waits for a restart or was kept.
pub(super) fn remove() -> io::Result<Elevated> {
    let left = cleanup::remove_all()?;
    if !left.is_complete() {
        cleanup::report(&left);
    }
    Ok(left.outcome())
}
