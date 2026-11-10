//! IW8b orchestration: discovery, hold, upgrade prep, the one child, verification and close.

use std::io;
use std::path::Path;

use crate::flow::{Completion, Failure, InstallAction, Request, Shortcuts};
use crate::host::Host;
use crate::mode::{Mode, Why, mode};
use crate::payload::Payload;
use crate::runner::{InstallError, Installed, Progress, Registry, Session};
use crate::syntax::FolderProblem;
use crate::upgrade::Staged;

/// The values a held session needs before extraction and spawning.
type Prepared = (Mode, Session, Option<Staged>, Option<[u8; 32]>, bool);

use super::steps;

/// Runs W8b's eleven steps; success closes the session and deletes no installed file.
#[expect(
    clippy::too_many_arguments,
    reason = "the request, payload, log, host and progress are all inputs"
)]
pub fn install(
    request: Request,
    payload: &Payload,
    velopack_log: &Path,
    host: &mut dyn Host,
    progress: &mut dyn FnMut(Progress),
) -> Result<Installed, InstallError> {
    run(request, payload, velopack_log, host, progress, None)
}

/// Retries one retained empty fresh root without another discovery or pin.
#[expect(
    clippy::too_many_arguments,
    reason = "the retained session, payload, log, host and progress are all inputs"
)]
pub fn retry(
    session: Session,
    payload: &Payload,
    velopack_log: &Path,
    host: &mut dyn Host,
    progress: &mut dyn FnMut(Progress),
) -> Result<Installed, InstallError> {
    let request = Request {
        mode: Box::new(Mode::Fresh),
        root: session.root.clone(),
        shortcuts: Shortcuts {
            desktop: true,
            start_menu: true,
        },
        action: InstallAction::RetryEmpty,
    };
    run(
        request,
        payload,
        velopack_log,
        host,
        progress,
        Some(session),
    )
}

/// Runs one install, optionally resuming the retained empty root of a failed fresh run.
#[expect(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "the eleven-step order is the contract and stays readable in one place"
)]
fn run(
    request: Request,
    payload: &Payload,
    velopack_log: &Path,
    host: &mut dyn Host,
    progress: &mut dyn FnMut(Progress),
    resume: Option<Session>,
) -> Result<Installed, InstallError> {
    let (found, mut session, staged, update_sha256, upgrade) = match resume {
        Some(session) => resume_session(request, session, host)?,
        None => start_session(&request, payload, host)?,
    };

    // Step 6 (fresh): extract Setup.exe into the private work folder.
    if !upgrade {
        session.extracted = match host.extract(payload) {
            Ok(extracted) => Some(extracted),
            Err(_) => {
                steps::close_with(session, host, true);
                return Err(InstallError::Extract);
            }
        };
    }

    // Step 7: recheck; fresh installs also prove the held root is still empty and no HKCU key appeared.
    if !steps::unchanged(host, &session.pin) {
        steps::close_with(session, host, !upgrade);
        return Err(InstallError::Failed(Failure::Changed));
    }
    if !upgrade && !steps::empty(host, &session.pin) {
        steps::close_with(session, host, true);
        return Err(InstallError::Failed(Failure::Changed));
    }
    if upgrade && !steps::clear(host, &session.root) {
        steps::close_with(session, host, false);
        return Err(InstallError::Blocked);
    }
    if !upgrade && !matches!(host.registry_root(&session.pin), Ok(Registry::Absent)) {
        steps::close_with(session, host, true);
        return Err(InstallError::ModeChanged(Box::new(Mode::Ambiguous(
            Why::MultipleInstalls,
        ))));
    }

    let args = steps::spawn_args(
        &found,
        &session.root,
        session.extracted.as_ref(),
        staged.as_ref(),
        velopack_log,
    );
    let Some(args) = args else {
        steps::close_with(session, host, !upgrade);
        return Err(InstallError::Failed(Failure::Upgrade(None)));
    };

    // Step 8: one child, waited to the end; a wait error still waits before any cleanup.
    let mut child = match host.spawn(&args) {
        Ok(child) => child,
        Err(_) => return Err(steps::fail_after_child(session, host, None)),
    };
    let code = match steps::wait(&mut *child, progress) {
        Ok(code) => code,
        Err(_) => return Err(steps::fail_after_child(session, host, None)),
    };

    // Step 9: exit code 0, files, chain and registry entry.
    if code != 0 {
        return Err(steps::fail_after_child(
            session,
            host,
            Some(child_code(code)),
        ));
    }
    let verified = match host.verify_files(&session.root, update_sha256) {
        Ok(verified) => verified,
        Err(_) => return Err(steps::fail_after_child(session, host, Some(0))),
    };
    if !steps::files_ok(&verified, payload) {
        return Err(steps::fail_after_child(session, host, Some(0)));
    }
    if !steps::unchanged(host, &session.pin) {
        return Err(steps::fail_after_exit(
            session,
            Failure::AfterExit(Some(0)),
            host,
        ));
    }
    if !matches!(host.registry_root(&session.pin), Ok(Registry::NamesRoot)) {
        return Err(InstallError::Unlisted(session));
    }

    // Step 10: the marker, written only after verification and never overwritten.
    let root = session.root.clone();
    let shortcuts = session.shortcuts;
    let legacy_marker_failed = if upgrade {
        steps::upgrade_marker(session, payload, host, child_code(code))?
    } else {
        steps::write_fresh_marker(session, payload, host)?;
        let _ = host.remove_shortcuts(&root, shortcuts.desktop, shortcuts.start_menu);
        false
    };

    // Step 11: the marker helpers close the session; success deletes nothing installed.
    Ok(Installed {
        completion: Completion::Installed,
        legacy_marker_failed,
    })
}

/// Converts a child exit code to the unsigned code IW17 prints, keeping every bit.
///
/// Windows crash codes such as `0xC0000005` arrive as negative `i32` values.
fn child_code(code: i32) -> u32 {
    code.cast_unsigned()
}

/// Steps 1-5 for a normal run: elevation, discovery, hold, recheck and upgrade preparation.
#[expect(
    clippy::too_many_lines,
    reason = "the discovery, hold and upgrade preparation stay together"
)]
fn start_session(
    request: &Request,
    payload: &Payload,
    host: &mut dyn Host,
) -> Result<Prepared, InstallError> {
    if request.action == InstallAction::MarkAgain {
        return Err(InstallError::WrongAction);
    }
    let found = discover(host, payload, request)?;
    let root = match &found {
        Mode::Upgrade(install) => install.root.clone(),
        _ => request.root.clone(),
    };
    let upgrade = matches!(found, Mode::Upgrade(_));
    let space = payload.space();
    let pin = hold(host, &found, &root, &space)?;
    if !steps::unchanged(host, &pin) {
        let _ = host.cleanup(pin, None, true);
        return Err(InstallError::Failed(Failure::Changed));
    }

    let mut staged = None;
    let mut update_sha256 = None;
    if let Mode::Upgrade(_) = &found {
        match host.writable(&pin) {
            Ok(true) => {}
            _ => {
                let _ = host.cleanup(pin, None, false);
                return Err(InstallError::Failed(Failure::Folder(
                    FolderProblem::NotWritable,
                )));
            }
        }
        if !steps::clear(host, &root) {
            let _ = host.cleanup(pin, None, false);
            return Err(InstallError::Blocked);
        }
        staged = Some(match host.stage_package(&pin, payload) {
            Ok(staged) => staged,
            Err(_) => {
                let _ = host.cleanup(pin, None, false);
                return Err(InstallError::Failed(Failure::Upgrade(None)));
            }
        });
        update_sha256 = Some(match host.replace_update_exe(&pin, payload) {
            Ok(hash) => hash,
            Err(_) => {
                let _ = host.cleanup(pin, None, false);
                return Err(InstallError::Failed(Failure::Upgrade(None)));
            }
        });
    }

    let discovery_marker = match &found {
        Mode::Upgrade(install) => install
            .marker
            .as_ref()
            .and_then(|marker| marker.as_ref().ok())
            .cloned(),
        _ => None,
    };
    let session = Session {
        pin,
        extracted: None,
        marker: None,
        marker_id: None,
        root,
        upgrade,
        shortcuts: request.shortcuts,
        discovery_marker,
    };
    Ok((found, session, staged, update_sha256, upgrade))
}

/// Validates one retained empty root and returns it ready for a second Setup.exe.
fn resume_session(
    request: Request,
    mut session: Session,
    host: &mut dyn Host,
) -> Result<Prepared, InstallError> {
    if request.action != InstallAction::RetryEmpty || session.upgrade {
        steps::close_with(session, host, false);
        return Err(InstallError::WrongAction);
    }
    let empty = steps::empty(host, &session.pin);
    if !steps::unchanged(host, &session.pin) || !empty {
        steps::close_with(session, host, true);
        return Err(InstallError::Failed(Failure::Changed));
    }
    session.extracted = None;
    session.marker = None;
    session.marker_id = None;
    Ok((Mode::Fresh, session, None, None, false))
}

/// Steps 1-2: refuses elevation, then selects the current mode from discovery.
fn discover(
    host: &mut dyn Host,
    payload: &Payload,
    request: &Request,
) -> Result<Mode, InstallError> {
    match host.elevated() {
        Ok(false) => {}
        _ => return Err(InstallError::Elevated),
    }
    let discovery = host
        .discover()
        .map_err(|_| InstallError::ModeChanged(Box::new(Mode::Ambiguous(Why::InvalidFolder))))?;
    let found = mode(&discovery, &payload.version);
    match &found {
        Mode::Fresh | Mode::Upgrade(_) => {}
        _ => return Err(InstallError::ModeChanged(Box::new(found))),
    }
    ensure_request(request, &found)?;
    Ok(found)
}

/// Refuses a fresh install while an install exists and an upgrade whose identity moved.
fn ensure_request(request: &Request, found: &Mode) -> Result<(), InstallError> {
    match (request.mode.as_ref(), found) {
        (Mode::Fresh, Mode::Fresh) => Ok(()),
        (Mode::Fresh, Mode::Upgrade(install)) => {
            Err(InstallError::AlreadyInstalled(install.root.clone()))
        }
        (Mode::Upgrade(want), Mode::Upgrade(now))
            if want.root == now.root && want.identity == now.identity =>
        {
            Ok(())
        }
        _ => Err(InstallError::ModeChanged(Box::new(found.clone()))),
    }
}

/// Step 3: creates and holds a fresh chain, or holds the discovered root.
fn hold(
    host: &mut dyn Host,
    found: &Mode,
    root: &Path,
    space: &crate::placement::Space,
) -> Result<super::Pin, InstallError> {
    let result = match found {
        Mode::Upgrade(install) => host.pin_existing(root, install.identity),
        _ => host.pin_fresh(root, space),
    };
    result.map_err(|error| {
        let problem = error
            .get_ref()
            .and_then(|error| error.downcast_ref::<crate::host::FolderRefusal>())
            .map(|refusal| refusal.0)
            .or_else(|| {
                (error.kind() == io::ErrorKind::AlreadyExists).then_some(FolderProblem::RootExists)
            })
            .unwrap_or(FolderProblem::ProbeFailed);
        InstallError::Failed(Failure::Folder(problem))
    })
}

/// Closes a retained session and removes its empty created folders on Close.
pub fn close(session: Session, host: &mut dyn Host) {
    let remove_created = !session.upgrade;
    steps::close_with(session, host, remove_created);
}

/// Closes a session kept after Setup.exe exited 0; it only drops handles and deletes nothing.
pub fn close_installed(session: Session, host: &mut dyn Host) {
    steps::close_success(session, host);
}

/// Retries a failed marker write on the session's held root.
pub fn mark_again(mut session: Session, host: &mut dyn Host) -> Result<(), InstallError> {
    if !steps::unchanged(host, &session.pin) {
        return Err(steps::fail_after_exit(
            session,
            Failure::AfterExit(None),
            host,
        ));
    }
    let Some(marker) = session.marker.clone() else {
        return Err(InstallError::Unmarked(session));
    };
    match host.write_marker(&session.pin, &marker) {
        Ok(id) => {
            session.marker_id = Some(id);
            steps::close_success(session, host);
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            Err(InstallError::Remarked(session))
        }
        Err(_) => Err(InstallError::Unmarked(session)),
    }
}
