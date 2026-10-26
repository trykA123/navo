#![allow(unsafe_code, reason = "SCM installation requires Win32 FFI")]

use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    ERROR_SERVICE_ALREADY_RUNNING, ERROR_SERVICE_DOES_NOT_EXIST, ERROR_SERVICE_EXISTS,
    ERROR_SERVICE_NOT_ACTIVE,
};
use windows_sys::Win32::Storage::FileSystem::DELETE;
use windows_sys::Win32::System::Services::*;
use windows_sys::Win32::UI::Shell::{FOLDERID_ProgramData, FOLDERID_ProgramFiles};

use super::directory::{Policy, ProtectedDirectory, known_folder};
use super::elevation::wide;
use super::install_plan::{
    CopyPlan, FERRITE, FreshInstall, NAVO, Product, SettingsBackup, Step, clear_directory,
    data_directories, run_install,
};
use super::policy::{FAILURE_DELAYS, RESET_SECONDS, RESTART_CLEAN_STOPS};
use super::scm::{
    ServiceHandle, checked, open_manager, service_handle, service_name, service_name_of,
};
use super::storage::Storage;
use super::upgrade::{UpgradeStep, await_answer, remove_both, run_upgrade};

mod configuration;
mod legacy;

const ANSWER_WAIT: (Duration, Duration) = (Duration::from_secs(30), Duration::from_millis(250));

pub(super) fn install() -> io::Result<()> {
    let legacy = legacy::detect()?;
    let config = match legacy::carried_config() {
        Some(config) => config,
        None => super::session::installation_config()?,
    };
    let leftover = run_upgrade(legacy, |step| match step {
        UpgradeStep::InstallNew => install_current(&config),
        UpgradeStep::AwaitNew => {
            await_answer(ANSWER_WAIT, || rusty_index::pipe::Client::connect().is_ok())
        }
        UpgradeStep::RemoveLegacyService => remove_service(FERRITE),
        UpgradeStep::RemoveLegacyFiles => remove_files(FERRITE),
    })?;
    if let Some(warning) = leftover {
        tracing::warn!(%warning, "index service installed with old files left in place");
        eprintln!("navo: index service: warning: {warning}");
    }
    Ok(())
}

fn install_current(config: &super::config::Config) -> io::Result<()> {
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
    let result = install_with(&plan, config, &mut fresh);
    if result.is_err() {
        let _ = fresh.remove_directories();
    }
    result
}

fn install_with(
    plan: &CopyPlan,
    config: &super::config::Config,
    fresh: &mut FreshInstall,
) -> io::Result<()> {
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
    let mut settings = None;
    let result = run_install(exists, was_running, |step| match step {
        Step::RecoverOld => fresh.recover_executable(plan),
        Step::Stop => stop(&service),
        Step::Copy => plan.copy_from(&std::env::current_exe()?),
        Step::RenameOld => plan.rename_old(),
        Step::Replace => plan.replace(),
        Step::BackupSettings => {
            settings = Some(SettingsBackup::capture(
                &storage.directory.join("service.json"),
            )?);
            Ok(())
        }
        Step::Configure => update(&service, &command)
            .and_then(|()| configure_recovery(&service))
            .and_then(|()| storage.write_config(config)),
        Step::Start => start(&service),
        Step::Restore => plan.restore(),
        Step::RestoreSettings => settings.as_ref().map_or(Ok(()), SettingsBackup::restore),
        Step::RemoveFresh => fresh.remove_executable(plan),
        Step::DiscardCopy => plan.discard_copy(),
        Step::RestoreConfiguration => match &previous {
            Some(previous) => previous.restore(&service),
            None => Ok(()),
        },
        Step::DeleteOld => plan.delete_old(),
    });
    finish_install(result, exists, &service)
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
    let mut command = vec![u16::from(b'"')];
    command.extend(plan.executable.as_os_str().encode_wide());
    command.extend(wide("\" --index-service run"));
    command
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

pub(super) fn remove() -> io::Result<()> {
    remove_both([NAVO, FERRITE], |product| {
        super::removal::run(|| remove_service(product), || remove_files(product))
    })
}

fn remove_files(product: Product) -> io::Result<()> {
    let binary = remove_binary(product);
    binary.and(remove_data(product))
}

fn remove_service(product: Product) -> io::Result<()> {
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
    stop(&service)?;
    // SAFETY: service has DELETE access and is stopped.
    checked(unsafe { DeleteService(service.0) })?;
    drop(service);
    Ok(())
}

fn remove_binary(product: Product) -> io::Result<()> {
    let plan = CopyPlan::of(
        product,
        &known_folder(&FOLDERID_ProgramFiles)?,
        std::process::id(),
    );
    let parent = plan
        .directory
        .parent()
        .ok_or_else(|| io::Error::other("missing install parent"))?;
    let Some(mut guard) = open_existing(&[parent, &plan.directory], Policy::Executable)? else {
        return Ok(());
    };
    super::removal::remove_image(|| std::fs::remove_file(&plan.executable))?;
    let _ = plan.delete_old();
    guard.remove_last(&plan.directory)?;
    guard.remove_last(parent)
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

fn stop(service: &ServiceHandle) -> io::Result<()> {
    let mut status = SERVICE_STATUS::default();
    // SAFETY: service is live with QUERY_STATUS access; status is writable.
    checked(unsafe { QueryServiceStatus(service.0, &mut status) })?;
    if !matches!(
        status.dwCurrentState,
        SERVICE_STOPPED | SERVICE_STOP_PENDING
    ) {
        // SAFETY: service has STOP access and status is writable output.
        if unsafe { ControlService(service.0, SERVICE_CONTROL_STOP, &mut status) } == 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(ERROR_SERVICE_NOT_ACTIVE as i32) {
                return Err(error);
            }
        }
    }
    let started = Instant::now();
    while status.dwCurrentState != SERVICE_STOPPED {
        if started.elapsed() >= Duration::from_secs(60) {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "index service did not stop",
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
        // SAFETY: service remains live and status is writable output.
        checked(unsafe { QueryServiceStatus(service.0, &mut status) })?;
    }
    Ok(())
}
