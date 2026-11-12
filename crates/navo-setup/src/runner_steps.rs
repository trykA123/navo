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
use crate::progress_log::PackageWeights;
use crate::progress_tail::LogTailer;
use crate::runner::{Child, InstallError, Progress, Session, Verified};
use crate::upgrade::{Staged, apply_args};

/// IW4's 180-second deadline, after which only the progress text changes.
const STILL_WORKING_SECONDS: u64 = 180;

/// Builds IW3's argument array for the current mode, with D117's `--verbose`.
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
                "--verbose".into(),
            ])
        }
    }
}

/// The child's log, with the mode and weights the estimator needs to weigh it.
pub(super) struct WatchedLog<'a> {
    pub path: &'a Path,
    pub upgrade: bool,
    pub weights: Option<PackageWeights>,
}

/// Waits in `poll_step` ticks, tailing the log when one is given. An event goes out only when the
/// whole seconds, the percentage or the phase changed, so a silent wait still reports once a second.
pub(super) fn wait_with_log(
    child: &mut dyn Child,
    log: Option<WatchedLog<'_>>,
    poll_step: Duration,
    progress: &mut dyn FnMut(Progress),
) -> io::Result<i32> {
    let mut tailer = log.map(|log| LogTailer::new(log.path, log.upgrade, log.weights));
    let mut elapsed = Duration::ZERO;
    let mut last = Progress::from_elapsed(Duration::ZERO, false);
    loop {
        match child.wait_timeout(poll_step) {
            Ok(Some(code)) => return Ok(code),
            Ok(None) => {}
            Err(error) => return child.wait().map_err(|_| error),
        }
        elapsed = elapsed.saturating_add(poll_step);
        let (percent, stage) = match tailer.as_mut() {
            Some(tailer) => tailer.snapshot(elapsed),
            None => (None, None),
        };
        let seconds = Duration::from_secs(elapsed.as_secs());
        let event = Progress {
            elapsed: seconds,
            still_working: seconds.as_secs() >= STILL_WORKING_SECONDS,
            percent,
            stage,
        };
        if event != last {
            progress(event);
            last = event;
        }
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

/// Cleans up after a failed child; only a nonzero exit retains an empty fresh root for Retry.
pub(super) fn fail_after_child(
    mut session: Session,
    host: &mut dyn Host,
    code: Option<u32>,
) -> InstallError {
    if session.upgrade {
        close_success(session, host);
        return InstallError::Failed(Failure::Upgrade(code));
    }
    if code != Some(0) && matches!(host.root_empty(&session.pin), Ok(true)) {
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

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::progress_log::InstallStage;
    use crate::runner_fake::FakeHost;

    const HEADER: &str =
        "[Setup:7] [10:00:00] [INFO] Extracting 2 app files to \"C:\\N\\current\"...\n";
    const FIRST: &str =
        "[Setup:7] [10:00:01] [DEBUG]     0 Extracting 'lib/net8/navo.exe' to \"C:\\n.exe\"\n";
    const BACKING_UP: &str = "[Setup:7] [10:00:02] [INFO] Backing up current dir\n";

    /// A child that times out `ticks` times and then exits 0.
    fn child(ticks: usize) -> Box<dyn Child> {
        let mut host = FakeHost::new();
        for _ in 0..ticks {
            host.child_script.push_back(None);
        }
        host.child_script.push_back(Some(0));
        host.spawn(&[]).expect("spawn")
    }

    #[test]
    fn the_timed_wait_shows_a_percentage_then_pulses_when_finalizing_stalls() {
        let dir = tempfile::tempdir().expect("temp");
        let log = dir.path().join("velopack.log");
        fs::write(&log, format!("{HEADER}{FIRST}")).expect("write");
        let mut events = Vec::new();
        let code = wait_with_log(
            &mut *child(32),
            Some(WatchedLog {
                path: &log,
                upgrade: true,
                weights: None,
            }),
            Duration::from_millis(250),
            &mut |event| {
                if event.elapsed == Duration::from_secs(2) {
                    fs::write(&log, format!("{HEADER}{FIRST}{BACKING_UP}")).expect("append");
                }
                events.push(event);
            },
        )
        .expect("wait");
        assert_eq!(code, 0);
        let shown = events
            .iter()
            .position(|event| event.percent == Some(94))
            .expect("the folder swap must reach the callback");
        assert_eq!(events[shown].stage, Some(InstallStage::Finalizing));
        let pulsed = events
            .iter()
            .position(|event| event.percent.is_none())
            .expect("a stalled Finalizing must fall back to the pulse");
        assert!(pulsed > shown, "the pulse must follow the percentage");
        assert_eq!(events[pulsed].stage, Some(InstallStage::Finalizing));
        assert!(events.iter().all(|event| event.percent != Some(100)));
        assert_eq!(
            events.last().expect("events").elapsed,
            Duration::from_secs(8)
        );
    }

    #[test]
    fn a_wait_without_a_log_reports_once_a_second_and_no_percentage() {
        let mut events = Vec::new();
        wait_with_log(&mut *child(3), None, Duration::from_secs(1), &mut |event| {
            events.push(event)
        })
        .expect("wait");
        assert_eq!(events.len(), 3);
        assert_eq!(events[2].elapsed, Duration::from_secs(3));
        assert!(
            events
                .iter()
                .all(|event| event.percent.is_none() && event.stage.is_none())
        );
    }
}
