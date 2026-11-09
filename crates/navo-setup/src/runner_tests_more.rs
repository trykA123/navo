//! Remaining W8b fake-host bullets, split out to keep each test file small.

use super::*;

/// Running scans and file checks fail closed with IW17's codes for both modes.
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one test covers both running checks and both sq.version failures"
)]
fn a_running_navo_a_scan_error_and_bad_sq_version_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());

    let mut host = upgrade_host("0.3.0", true);
    host.running.clear();
    host.running.push_back(Ok(true));
    let (result, _) = run(&mut host, upgrade_request("0.3.0", true), &payload);
    let error = result.err().unwrap();
    assert!(matches!(error, InstallError::Blocked));
    assert_eq!(error.exit_code(), 4);
    assert_eq!(count(&host, |call| matches!(call, Call::Extract)), 0);
    assert_eq!(count(&host, |call| matches!(call, Call::Stage)), 0);
    assert_eq!(count(&host, |call| matches!(call, Call::Spawn(_))), 0);

    let mut host = upgrade_host("0.3.0", true);
    host.running.clear();
    host.running.extend([Ok(false), Ok(true)]);
    let (result, _) = run(&mut host, upgrade_request("0.3.0", true), &payload);
    let error = result.err().unwrap();
    assert!(matches!(error, InstallError::Blocked));
    assert_eq!(error.exit_code(), 4);
    assert_eq!(count(&host, |call| matches!(call, Call::Stage)), 1);
    assert_eq!(count(&host, |call| matches!(call, Call::Spawn(_))), 0);

    let mut host = upgrade_host("0.3.0", true);
    host.running.clear();
    host.running.push_back(Err(io::Error::other("scan failed")));
    let (result, _) = run(&mut host, upgrade_request("0.3.0", true), &payload);
    let error = result.err().unwrap();
    assert!(matches!(error, InstallError::Blocked));
    assert_eq!(error.exit_code(), 4);
    assert_eq!(count(&host, |call| matches!(call, Call::Stage)), 0);

    for version in [None, Some(sq("0.3.0"))] {
        let mut host = fresh_host();
        host.verified.clear();
        host.verified.push_back(Verified {
            update_exe: true,
            sq_version: version.clone(),
        });
        let (result, _) = run(&mut host, fresh_request(), &payload);
        let error = result.err().unwrap();
        assert_eq!(error.exit_code(), 5);
        assert_eq!(count(&host, |call| matches!(call, Call::WriteMarker(_))), 0);

        let mut host = upgrade_host("0.3.0", true);
        host.verified.clear();
        host.verified.push_back(Verified {
            update_exe: true,
            sq_version: version,
        });
        let (result, _) = run(&mut host, upgrade_request("0.3.0", true), &payload);
        let error = result.err().unwrap();
        assert!(matches!(
            error,
            InstallError::Failed(Failure::Upgrade(Some(0)))
        ));
        assert_eq!(error.exit_code(), 5);
    }
}

/// Same and Newer hold, write and spawn nothing: the downgrade guard.
#[test]
fn same_and_newer_hold_write_and_spawn_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());
    for version in ["0.4.0", "0.5.0"] {
        let found = mode(
            &upgrade_discovery(version, true),
            &parse_version(VERSION).unwrap(),
        );
        let mut host = FakeHost::new();
        host.discoveries
            .push_back(Ok(upgrade_discovery(version, true)));
        let (result, _) = run(
            &mut host,
            request(found, UPGRADE_ROOT, InstallAction::Run),
            &payload,
        );
        let error = result.err().unwrap();
        assert!(matches!(error, InstallError::ModeChanged(_)));
        assert_eq!(error.exit_code(), 8);
        for call in [
            Call::PinFresh(PathBuf::new()),
            Call::PinExisting(PathBuf::new(), identity()),
        ] {
            let kind = std::mem::discriminant(&call);
            assert_eq!(
                count(&host, |actual| std::mem::discriminant(actual) == kind),
                0
            );
        }
        assert_eq!(count(&host, |call| matches!(call, Call::Spawn(_))), 0);
        assert_eq!(count(&host, |call| matches!(call, Call::WriteMarker(_))), 0);
    }
}

/// A fresh install is refused while an install exists, and when a key appears before spawn.
#[test]
fn a_fresh_installto_while_a_key_or_the_legacy_folder_exists() {
    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());
    for marked in [true, false] {
        let mut host = FakeHost::new();
        host.discoveries
            .push_back(Ok(upgrade_discovery("0.3.0", marked)));
        let (result, _) = run(&mut host, fresh_request(), &payload);
        let error = result.err().unwrap();
        assert!(matches!(error, InstallError::AlreadyInstalled(_)));
        assert_eq!(error.exit_code(), 3);
        assert_eq!(count(&host, |call| matches!(call, Call::PinFresh(_))), 0);
    }

    let mut host = fresh_host();
    host.registry.clear();
    host.registry
        .push_back(Registry::NamesOther(UPGRADE_ROOT.into()));
    let (result, _) = run(&mut host, fresh_request(), &payload);
    let error = result.err().unwrap();
    assert!(matches!(error, InstallError::ModeChanged(_)));
    assert_eq!(error.exit_code(), 6);
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

/// An empty failed root is retained for Retry; a partial root is kept with Retry off.
#[test]
fn a_failed_setup_that_wrote_nothing_retains_the_root_and_one_that_wrote_a_file_keeps_it() {
    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());

    let mut host = fresh_host();
    host.root_empty.push_back(Ok(true));
    exit_with(&mut host, 1);
    let (result, _) = run(&mut host, fresh_request(), &payload);
    let Err(InstallError::RetryEmpty(session, _)) = result else {
        panic!("expected RetryEmpty");
    };
    assert_eq!(session.root(), Path::new(ROOT));
    assert_eq!(count(&host, |call| matches!(call, Call::Cleanup { .. })), 0);
    close(session, &mut host);
    assert!(host.calls.iter().any(|call| matches!(
        call,
        Call::Cleanup {
            remove_created: true
        }
    )));

    let mut host = fresh_host();
    exit_with(&mut host, 1);
    let (result, _) = run(&mut host, fresh_request(), &payload);
    let error = result.err().unwrap();
    assert!(matches!(
        error,
        InstallError::Failed(Failure::PartialRoot(Some(1)))
    ));
    assert_eq!(error.exit_code(), 5);
    assert!(host.calls.iter().any(|call| matches!(
        call,
        Call::Cleanup {
            remove_created: false
        }
    )));
}

/// Exit-0 marker and registry failures keep the session, and Close deletes nothing.
#[test]
fn after_exit_zero_marker_and_registry_failures_keep_the_session() {
    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());

    let mut host = fresh_host();
    host.marker_writes.push_back(MarkerWrite::Fail);
    let (result, _) = run(&mut host, fresh_request(), &payload);
    let Err(InstallError::Unmarked(session)) = result else {
        panic!("expected Unmarked");
    };
    assert_eq!(count(&host, |call| matches!(call, Call::Cleanup { .. })), 0);
    host.marker_writes.push_back(MarkerWrite::Ok);
    assert!(mark_again(session, &mut host).is_ok());
    assert_eq!(count(&host, |call| matches!(call, Call::WriteMarker(_))), 2);
    let markers: Vec<_> = host
        .calls
        .iter()
        .filter_map(|call| match call {
            Call::WriteMarker(marker) => Some(marker.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(markers[0].install_id, markers[1].install_id);
    assert_eq!(count(&host, |call| matches!(call, Call::Cleanup { .. })), 1);

    let mut host = fresh_host();
    host.marker_writes.push_back(MarkerWrite::Exists);
    let (result, _) = run(&mut host, fresh_request(), &payload);
    let Err(InstallError::Remarked(session)) = result else {
        panic!("expected Remarked");
    };
    assert_eq!(count(&host, |call| matches!(call, Call::Cleanup { .. })), 0);
    drop(session);
    assert_eq!(count(&host, |call| matches!(call, Call::Cleanup { .. })), 0);

    let mut host = fresh_host();
    host.registry.clear();
    host.registry.push_back(Registry::Absent);
    host.registry.push_back(Registry::Absent);
    let (result, _) = run(&mut host, fresh_request(), &payload);
    let Err(InstallError::Unlisted(session)) = result else {
        panic!("expected Unlisted");
    };
    assert_eq!(count(&host, |call| matches!(call, Call::WriteMarker(_))), 0);
    assert!(
        session
            .uninstall_command()
            .contains("Update.exe\" uninstall")
    );
    drop(session);
    assert_eq!(count(&host, |call| matches!(call, Call::Cleanup { .. })), 0);
}

/// A legacy upgrade whose marker write fails still counts, because its identity keeps it valid.
#[test]
fn a_legacy_upgrade_whose_marker_write_fails_still_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());
    let mut host = upgrade_host("0.3.0", false);
    host.markers.clear();
    host.markers.push_back(None);
    host.marker_writes.push_back(MarkerWrite::Fail);
    let (result, _) = run(&mut host, upgrade_request("0.3.0", false), &payload);
    assert!(matches!(
        result,
        Ok(Installed {
            legacy_marker_failed: true,
            ..
        })
    ));
    assert_eq!(count(&host, |call| matches!(call, Call::WriteMarker(_))), 1);
    assert!(host.calls.iter().any(|call| matches!(
        call,
        Call::Cleanup {
            remove_created: false
        }
    )));
}

/// A Retry reuses the retained session and its pin, so no second pin_fresh runs.
#[test]
fn a_retry_that_reuses_the_sessions_empty_root() {
    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());
    let mut host = fresh_host();
    host.root_empty.push_back(Ok(true));
    exit_with(&mut host, 1);
    let (result, _) = run(&mut host, fresh_request(), &payload);
    let Err(InstallError::RetryEmpty(session, _)) = result else {
        panic!("expected RetryEmpty");
    };
    assert_eq!(count(&host, |call| matches!(call, Call::PinFresh(_))), 1);

    host.registry.clear();
    host.registry.push_back(Registry::Absent);
    host.registry.push_back(Registry::NamesRoot);
    host.root_empty.push_back(Ok(true));
    host.child_script.clear();
    host.child_script.push_back(Some(0));
    let mut progress = Vec::new();
    let result = retry(session, &payload, Path::new(LOG), &mut host, &mut |event| {
        progress.push(event)
    });
    assert!(
        matches!(result, Ok(Installed { .. })),
        "{result:?} {:?}",
        host.calls
    );
    assert_eq!(count(&host, |call| matches!(call, Call::PinFresh(_))), 1);
    assert_eq!(count(&host, |call| matches!(call, Call::Spawn(_))), 2);
}
