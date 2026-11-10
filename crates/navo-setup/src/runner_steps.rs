//! Runner helpers: argument arrays, the timed wait, marker writes and failure cleanup.

use std::ffi::OsString;
use std::io;
use std::path::Path;
use std::time::Duration;

use crate::flow::Failure;
use crate::host::Host;
use crate::marker::Marker;
use crate::mode::Mode;
use crate::payload::Payload;
use crate::runner::{Child, InstallError, Progress, Session, Verified};
use crate::upgrade::{Staged, apply_args};

/// IW4's 180-second deadline, after which only the progress text changes.
const STILL_WORKING_SECONDS: u64 = 180;

/// Builds IW3's exact argument array for the current mode.
#[expect(
    clippy::too_many_arguments,
    reason = "the mode, root, extracted payload, staged package and log are all inputs"
)]
pub(super) fn spawn_args(
    found: &Mode,
    root: &Path,
    extracted: Option<&crate::runner::Extracted>,
    staged: Option<&Staged>,
    velopack_log: &Path,
) -> Option<Vec<OsString>> {
    match found {
        Mode::Upgrade(_) => {
            let staged = staged?;
            Some(apply_args(&root.to_path_buf(), staged, velopack_log))
        }
        _ => {
            let setup = extracted?;
            Some(vec![
                setup.path.clone().into_os_string(),
                "--installto".into(),
                root.as_os_str().to_os_string(),
                "--silent".into(),
                "--log".into(),
                velopack_log.as_os_str().to_os_string(),
            ])
        }
    }
}

/// Waits one second at a time; a timed-wait error waits without a deadline before failing.
pub(super) fn wait(child: &mut dyn Child, progress: &mut dyn FnMut(Progress)) -> io::Result<i32> {
    let mut seconds = 0u64;
    loop {
        match child.wait_timeout(Duration::from_secs(1)) {
            Ok(Some(code)) => return Ok(code),
            Ok(None) => {}
            Err(error) => return child.wait().map_err(|_| error),
        }
        seconds = seconds.saturating_add(1);
        progress(Progress {
            elapsed: Duration::from_secs(seconds),
            still_working: seconds >= STILL_WORKING_SECONDS,
        });
    }
}

/// Whether checks 2 and 3 hold: a regular Update.exe and the payload's sq.version.
pub(super) fn files_ok(verified: &Verified, payload: &Payload) -> bool {
    verified.update_exe
        && verified
            .sq_version
            .as_ref()
            .is_some_and(|sq| sq.pack_id == "NavoApp" && sq.version == payload.version)
}

/// Writes a fresh install's marker; failures keep the session for the Failed page.
pub(super) fn write_fresh_marker(
    mut session: Session,
    payload: &Payload,
    host: &mut dyn Host,
) -> Result<(), InstallError> {
    let marker = marker_for(payload)?;
    match host.write_marker(&session.pin, &marker) {
        Ok(id) => {
            session.marker = Some(marker);
            session.marker_id = Some(id);
            close_success(session, host);
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            session.marker = Some(marker);
            Err(InstallError::Remarked(session))
        }
        Err(_) => {
            session.marker = Some(marker);
            Err(InstallError::Unmarked(session))
        }
    }
}

/// Checks a marked upgrade's installId, or writes a legacy marker; a failed legacy write is flagged.
pub(super) fn upgrade_marker(
    mut session: Session,
    payload: &Payload,
    host: &mut dyn Host,
    code: u32,
) -> Result<bool, InstallError> {
    if let Some(expected) = session.discovery_marker.clone() {
        return match host.read_marker(&session.root) {
            Ok(Some(marker))
                if marker.pack_id == expected.pack_id
                    && marker.install_id == expected.install_id =>
            {
                session.marker = Some(marker);
                close_success(session, host);
                Ok(false)
            }
            _ => {
                close_success(session, host);
                Err(InstallError::Failed(Failure::AfterExit(Some(code))))
            }
        };
    }
    let mut failed = false;
    match host.read_marker(&session.root) {
        Ok(Some(marker)) => session.marker = Some(marker),
        _ => match marker_for(payload) {
            Ok(marker) => match host.write_marker(&session.pin, &marker) {
                Ok(id) => {
                    session.marker = Some(marker);
                    session.marker_id = Some(id);
                }
                Err(_) => failed = true,
            },
            Err(_) => failed = true,
        },
    }
    close_success(session, host);
    Ok(failed)
}

/// Builds a fresh marker with a CSPRNG install id and the payload's version.
fn marker_for(payload: &Payload) -> Result<Marker, InstallError> {
    let install_id = new_install_id().map_err(|_| InstallError::NoPayload)?;
    Ok(Marker {
        pack_id: "NavoApp".to_owned(),
        install_id,
        created_by: format!("navo-setup {}", payload.version),
    })
}

/// Returns 32 lowercase hex characters from the system CSPRNG.
fn new_install_id() -> io::Result<String> {
    let bytes = random_bytes()?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Reads 16 bytes from the platform's cryptographic random source.
#[cfg(windows)]
fn random_bytes() -> io::Result<[u8; 16]> {
    use windows_sys::Win32::Security::Cryptography::{
        BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
    };
    let mut bytes = [0u8; 16];
    // SAFETY: the buffer is writable and the system-preferred RNG needs no algorithm handle.
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            bytes.as_mut_ptr(),
            16,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status < 0 {
        return Err(io::Error::other(format!(
            "BCryptGenRandom failed: 0x{status:08X}"
        )));
    }
    Ok(bytes)
}

/// Reads 16 bytes from `/dev/urandom`, used by the Linux tests.
#[cfg(not(windows))]
fn random_bytes() -> io::Result<[u8; 16]> {
    use std::io::Read;
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes)
}

/// Closes a session with an explicit cleanup mode; errors are ignored after the child exits.
pub(super) fn close_with(session: Session, host: &mut dyn Host, remove_created: bool) {
    let _ = host.cleanup(session.pin, session.extracted, remove_created);
}

/// Closes a successful install and deletes nothing installed.
pub(super) fn close_success(session: Session, host: &mut dyn Host) {
    close_with(session, host, false);
}

/// Cleans up after a failed child; an empty fresh root is retained for Retry instead.
pub(super) fn fail_after_child(
    mut session: Session,
    host: &mut dyn Host,
    code: Option<u32>,
) -> InstallError {
    if session.upgrade {
        close_success(session, host);
        return InstallError::Failed(Failure::Upgrade(code));
    }
    if matches!(host.root_empty(&session.pin), Ok(true)) {
        session.extracted = None;
        return InstallError::RetryEmpty(session, code);
    }
    close_success(session, host);
    InstallError::Failed(Failure::PartialRoot(code))
}

/// Cleans up after an exit-0 verification failure while keeping installed files.
pub(super) fn fail_after_exit(
    session: Session,
    failure: Failure,
    host: &mut dyn Host,
) -> InstallError {
    close_success(session, host);
    InstallError::Failed(failure)
}

/// Whether the held folders, ancestors and refused set all still match.
pub(super) fn unchanged(host: &mut dyn Host, pin: &super::Pin) -> bool {
    host.ancestors_unchanged(pin).unwrap_or(false)
}

/// Whether no Navo process runs from the root; a scan error counts as running.
pub(super) fn clear(host: &mut dyn Host, root: &Path) -> bool {
    matches!(host.running(root), Ok(false))
}

/// Whether the held fresh root still holds nothing; an error counts as not empty.
pub(super) fn empty(host: &mut dyn Host, pin: &super::Pin) -> bool {
    matches!(host.root_empty(pin), Ok(true))
}
