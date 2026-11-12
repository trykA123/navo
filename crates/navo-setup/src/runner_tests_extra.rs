//! Extra W8b fake-host bullets, split out to keep each test file small.

use super::*;

/// A read-only upgrade root and a failed Update.exe replace stop before apply.
#[test]
fn an_unwritable_root_and_a_failed_update_exe_replace_stop_before_apply() {
    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());

    let mut host = upgrade_host("0.3.0", true);
    host.writable.push_back(false);
    let (result, _) = run(&mut host, upgrade_request("0.3.0", true), &payload);
    let error = result.err().unwrap();
    assert!(matches!(
        error,
        InstallError::Failed(Failure::Folder(FolderProblem::NotWritable))
    ));
    assert_eq!(error.exit_code(), 3);
    assert_eq!(count(&host, |call| matches!(call, Call::Stage)), 0);
    assert_eq!(count(&host, |call| matches!(call, Call::Spawn(_))), 0);

    let mut host = upgrade_host("0.3.0", true);
    host.replace_ok = false;
    let (result, _) = run(&mut host, upgrade_request("0.3.0", true), &payload);
    let error = result.err().unwrap();
    assert!(matches!(
        error,
        InstallError::Failed(Failure::Upgrade(None))
    ));
    assert_eq!(error.exit_code(), 5);
    assert_eq!(count(&host, |call| matches!(call, Call::Replace)), 1);
    assert_eq!(count(&host, |call| matches!(call, Call::Spawn(_))), 0);
}

/// A held folder changed after the child exits fails after exit with code 5 and keeps files.
#[test]
fn a_held_folder_changed_after_exit_fails_with_code_5() {
    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());
    let mut host = fresh_host();
    host.held_changed_after_spawn = true;
    let (result, _) = run(&mut host, fresh_request(), &payload);
    let error = result.err().unwrap();
    assert!(matches!(
        error,
        InstallError::Failed(Failure::AfterExit(Some(0)))
    ));
    assert_eq!(error.exit_code(), 5);
    assert_eq!(count(&host, |call| matches!(call, Call::Spawn(_))), 1);
    assert_eq!(count(&host, |call| matches!(call, Call::WriteMarker(_))), 0);
    assert!(host.calls.iter().any(|call| matches!(
        call,
        Call::Cleanup {
            remove_created: false
        }
    )));
}

/// A timed-wait error calls Child::wait before any cleanup.
#[test]
fn a_wait_timeout_error_waits_for_the_child_before_cleanup() {
    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());
    let mut host = fresh_host();
    host.child_wait_error = true;
    host.child_wait_code = 0;
    let (result, _) = run(&mut host, fresh_request(), &payload);
    assert!(matches!(result, Ok(Installed { .. })));
    assert_eq!(host.wait_calls.get(), 1);
    assert_eq!(host.cleanup_wait_calls.get(), Some(1));
}

/// An elevated token or an unknown elevation check exits 7 before any hold.
#[test]
fn an_elevated_or_unknown_token_exits_7() {
    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());
    for answer in [Ok(true), Err(io::Error::other("token unknown"))] {
        let mut host = FakeHost::new();
        host.elevated.push_back(answer);
        let (result, _) = run(&mut host, fresh_request(), &payload);
        let error = result.err().unwrap();
        assert!(matches!(error, InstallError::Elevated));
        assert_eq!(error.exit_code(), 7);
        assert_eq!(count(&host, |call| matches!(call, Call::PinFresh(_))), 0);
    }
}

/// A discovery error fails closed with the ambiguous exit code 6.
#[test]
fn a_discovery_error_exits_6() {
    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());
    let mut host = FakeHost::new();
    host.discoveries
        .push_back(Err(io::Error::other("discovery failed")));
    let (result, _) = run(&mut host, fresh_request(), &payload);
    let error = result.err().unwrap();
    assert!(matches!(error, InstallError::ModeChanged(_)));
    assert_eq!(error.exit_code(), 6);
    assert_eq!(count(&host, |call| matches!(call, Call::PinFresh(_))), 0);
}

/// A marked upgrade whose installId changed after apply fails with code 5 and keeps files.
#[test]
fn a_marked_upgrade_with_another_install_id_fails_with_code_5() {
    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());
    let mut host = upgrade_host("0.3.0", true);
    let mut changed = marker();
    changed.install_id = "fedcba9876543210fedcba9876543210".to_owned();
    host.markers.clear();
    host.markers.push_back(Some(changed));
    let (result, _) = run(&mut host, upgrade_request("0.3.0", true), &payload);
    let error = result.err().unwrap();
    assert!(matches!(
        error,
        InstallError::Failed(Failure::AfterExit(Some(0)))
    ));
    assert_eq!(error.exit_code(), 5);
    assert_eq!(count(&host, |call| matches!(call, Call::Spawn(_))), 1);
    assert_eq!(count(&host, |call| matches!(call, Call::WriteMarker(_))), 0);
    assert!(host.calls.iter().any(|call| matches!(
        call,
        Call::Cleanup {
            remove_created: false
        }
    )));
}

/// A refused fresh folder exits 3 before extraction or a spawn.
#[test]
fn a_refused_folder_root_exits_3_before_anything_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());
    let mut host = fresh_host();
    host.pin_fresh_ok = false;
    let (result, _) = run(&mut host, fresh_request(), &payload);
    let error = result.err().unwrap();
    assert!(matches!(
        error,
        InstallError::Failed(Failure::Folder(FolderProblem::ProbeFailed))
    ));
    assert_eq!(error.exit_code(), 3);
    let space = host.pin_fresh_space.expect("space checked");
    assert_eq!(
        space.payload,
        payload.setup_size.saturating_add(payload.package_size)
    );
    assert_eq!(space.compressed, payload.package_size);
    assert!(space.extracted > 0);
    assert_eq!(count(&host, |call| matches!(call, Call::Extract)), 0);
    assert_eq!(count(&host, |call| matches!(call, Call::Spawn(_))), 0);
    assert_eq!(count(&host, |call| matches!(call, Call::Cleanup { .. })), 0);
}

/// A disk-full extraction exits 5 and removes only the empty folders it created.
#[test]
fn an_extract_failure_exits_5_and_cleans_up() {
    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());
    let mut host = fresh_host();
    host.extract_ok = false;
    let (result, _) = run(&mut host, fresh_request(), &payload);
    let error = result.err().unwrap();
    assert!(matches!(error, InstallError::Extract));
    assert_eq!(error.exit_code(), 5);
    assert_eq!(count(&host, |call| matches!(call, Call::Spawn(_))), 0);
    assert_eq!(
        count(&host, |call| matches!(
            call,
            Call::Cleanup {
                remove_created: true
            }
        )),
        1
    );
}

/// MarkAgain passed to install is refused with its own error, never a folder code.
#[test]
fn mark_again_cannot_be_started_through_install() {
    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());
    let mut host = fresh_host();
    let request = request(Mode::Fresh, ROOT, InstallAction::MarkAgain);
    let (result, _) = run(&mut host, request, &payload);
    let error = result.err().unwrap();
    assert!(matches!(error, InstallError::WrongAction));
    assert_eq!(error.exit_code(), 5);
    assert_eq!(count(&host, |call| matches!(call, Call::PinFresh(_))), 0);
}

/// A changed chain makes mark_again refuse and keeps the marker unwritten.
#[test]
fn mark_again_refuses_a_changed_chain() {
    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());
    let mut host = fresh_host();
    host.marker_writes.push_back(MarkerWrite::Fail);
    let (result, _) = run(&mut host, fresh_request(), &payload);
    let Err(InstallError::Unmarked(session)) = result else {
        panic!("expected Unmarked");
    };
    host.held_changed = true;
    let result = mark_again(session, &mut host);
    let error = result.err().unwrap();
    assert!(matches!(
        error,
        InstallError::Failed(Failure::AfterExit(None))
    ));
    assert_eq!(error.exit_code(), 5);
    assert_eq!(count(&host, |call| matches!(call, Call::WriteMarker(_))), 1);
}

/// An upgrade never removes or re-creates shortcuts (IW11).
#[test]
fn an_upgrade_never_removes_shortcuts() {
    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());
    let mut host = upgrade_host("0.3.0", true);
    let (result, _) = run(&mut host, upgrade_request("0.3.0", true), &payload);
    assert!(matches!(result, Ok(Installed { .. })));
    assert_eq!(
        count(&host, |call| matches!(call, Call::RemoveShortcuts { .. })),
        0
    );
}

/// A low-space placement refusal keeps NotEnoughSpace through the runner.
#[test]
fn a_low_space_refusal_keeps_its_folder_problem() {
    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());
    let mut host = fresh_host();
    host.pin_fresh_problem = Some(FolderProblem::NotEnoughSpace);
    let (result, _) = run(&mut host, fresh_request(), &payload);
    let error = result.err().unwrap();
    assert!(matches!(
        error,
        InstallError::Failed(Failure::Folder(FolderProblem::NotEnoughSpace))
    ));
    assert_eq!(error.exit_code(), 3);
    let space = host.pin_fresh_space.expect("space checked");
    assert_eq!(
        space.payload,
        payload.setup_size.saturating_add(payload.package_size)
    );
    assert_eq!(space.compressed, payload.package_size);
    assert!(space.extracted > 0);
    assert_eq!(count(&host, |call| matches!(call, Call::Extract)), 0);
    assert_eq!(count(&host, |call| matches!(call, Call::Spawn(_))), 0);
    assert_eq!(count(&host, |call| matches!(call, Call::Cleanup { .. })), 0);
}

/// A real Velopack log reaches the callback as a percentage, then as a pulse when Finalizing stalls.
#[test]
fn a_real_log_reaches_the_callback_as_a_percentage_then_a_pulse() {
    const HEADER: &str =
        "[Setup:7] [10:00:00] [INFO] Extracting 2 app files to \"C:\\N\\current\"...\n";
    const FIRST: &str =
        "[Setup:7] [10:00:01] [DEBUG]     0 Extracting 'lib/net8/navo.exe' to \"C:\\n.exe\"\n";
    const BACKING_UP: &str = "[Setup:7] [10:00:02] [INFO] Backing up current dir\n";

    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());
    let log = dir.path().join("velopack.log");
    std::fs::write(&log, format!("{HEADER}{FIRST}")).unwrap();
    let mut host = fresh_host();
    host.child_script.clear();
    for _ in 0..32 {
        host.child_script.push_back(None);
    }
    host.child_script.push_back(Some(0));
    let mut events = Vec::new();
    let result = install(fresh_request(), &payload, &log, &mut host, &mut |event| {
        if event.elapsed == std::time::Duration::from_secs(2) {
            std::fs::write(&log, format!("{HEADER}{FIRST}{BACKING_UP}")).unwrap();
        }
        events.push(event);
    });
    assert!(matches!(result, Ok(Installed { .. })));
    let stage = crate::progress_log::InstallStage::Finalizing;
    let shown = events
        .iter()
        .position(|event| event.percent == Some(94))
        .expect("the run must report the folder swap");
    assert_eq!(events[shown].stage, Some(stage));
    let pulsed = events
        .iter()
        .position(|event| event.percent.is_none() && event.stage == Some(stage))
        .expect("a stalled Finalizing must fall back to the pulse");
    assert!(pulsed > shown, "the pulse must follow the percentage");
    assert!(events.iter().all(|event| event.percent != Some(100)));
}
