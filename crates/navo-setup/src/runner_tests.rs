//! Fake-host tests for W8b's runner bullets; no Windows probe runs here.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use navo_core::{FileIdentity, FileIdentityKind};
use navo_update::parse_version;

use super::*;
use crate::flow::{Completion, Failure, InstallAction, Request, Shortcuts};
use crate::mode::{Discovery, Install, Mode, SqVersion, mode, parse_sq_version};
use crate::payload::Payload;
use crate::runner_fake::{Call, FakeHost, MarkerWrite, SETUP_PATH};
use crate::syntax::FolderProblem;

const ROOT: &str = r"C:\Programs\Navo";
const UPGRADE_ROOT: &str = r"C:\Apps\Navo";
const LOG: &str = r"C:\Temp\NavoSetup-logs\20261109T000000000Z-velopack.log";
const VERSION: &str = "0.4.0";

/// Builds a validated file payload with the crate's package name and version.
fn payload(root: &Path) -> Payload {
    let setup = root.join("input.exe");
    fs::write(&setup, b"fixture Setup.exe").unwrap();
    let package = root.join(format!("NavoApp-{VERSION}-full.nupkg"));
    let mut zip = zip::ZipWriter::new(fs::File::create(&package).unwrap());
    zip.start_file(
        "lib/net8/NavoApp.nuspec",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    zip.write_all(
        format!(
            "<package><metadata><id>NavoApp</id><version>{VERSION}</version></metadata></package>"
        )
        .as_bytes(),
    )
    .unwrap();
    zip.finish().unwrap();
    Payload::from_files(&setup, &package, parse_version(VERSION).unwrap()).unwrap()
}

/// Parses one sq.version identity.
fn sq(version: &str) -> SqVersion {
    parse_sq_version(&format!(
        "<package><metadata><id>NavoApp</id><version>{version}</version></metadata></package>"
    ))
    .unwrap()
}

/// Discovery's native identity for an upgrade root.
fn identity() -> FileIdentity {
    FileIdentity {
        volume: 1,
        file: [2; 16],
        kind: FileIdentityKind::Native,
    }
}

/// Builds one discovered install at `root` with a version and optional marker.
fn found_install(root: &str, version: &str, marker: Option<Marker>) -> Install {
    Install {
        root: PathBuf::from(root),
        identity: identity(),
        safe_chain: true,
        update_exe: true,
        sq_version: Ok(sq(version)),
        marker: marker.map(Ok),
    }
}

/// Builds a marked or legacy upgrade discovery at the fake upgrade root.
fn upgrade_discovery(version: &str, marked: bool) -> Discovery {
    let found = found_install(UPGRADE_ROOT, version, marked.then(marker));
    if marked {
        Discovery {
            registry: Some(Ok(found)),
            legacy: None,
        }
    } else {
        Discovery {
            registry: None,
            legacy: Some(found),
        }
    }
}

/// A valid marker for upgrade discovery.
fn marker() -> Marker {
    Marker {
        pack_id: "NavoApp".to_owned(),
        install_id: "0123456789abcdef0123456789abcdef".to_owned(),
        created_by: format!("navo-setup {VERSION}"),
    }
}

/// Builds a request from the discovered mode and an action.
fn request(mode: Mode, root: &str, action: InstallAction) -> Request {
    Request {
        mode: Box::new(mode),
        root: PathBuf::from(root),
        shortcuts: Shortcuts {
            desktop: true,
            start_menu: true,
        },
        action,
    }
}

/// Builds a fresh request whose mode matches the fake discovery.
fn fresh_request() -> Request {
    request(Mode::Fresh, ROOT, InstallAction::Run)
}

/// Builds an upgrade request whose mode matches the fake discovery.
fn upgrade_request(version: &str, marked: bool) -> Request {
    let found = mode(
        &upgrade_discovery(version, marked),
        &parse_version(VERSION).unwrap(),
    );
    request(found, UPGRADE_ROOT, InstallAction::Run)
}

/// Runs one install and collects every progress event.
fn run(
    host: &mut FakeHost,
    request: Request,
    payload: &Payload,
) -> (Result<Installed, InstallError>, Vec<Progress>) {
    let mut progress = Vec::new();
    let result = install(request, payload, Path::new(LOG), host, &mut |event| {
        progress.push(event)
    });
    (result, progress)
}

/// A host scripted for a successful fresh install.
fn fresh_host() -> FakeHost {
    let mut host = FakeHost::new();
    host.discoveries.push_back(Ok(Discovery::default()));
    host.registry.push_back(Registry::Absent);
    host.registry.push_back(Registry::NamesRoot);
    host.verified.push_back(Verified {
        update_exe: true,
        sq_version: Some(sq(VERSION)),
    });
    host.child_script.push_back(Some(0));
    host
}

/// A host scripted for a successful marked or legacy upgrade.
fn upgrade_host(version: &str, marked: bool) -> FakeHost {
    let mut host = FakeHost::new();
    host.discoveries
        .push_back(Ok(upgrade_discovery(version, marked)));
    host.running.push_back(Ok(false));
    host.running.push_back(Ok(false));
    host.registry.push_back(Registry::NamesRoot);
    host.markers.push_back(marked.then(marker));
    host.verified.push_back(Verified {
        update_exe: true,
        sq_version: Some(sq(VERSION)),
    });
    host.child_script.push_back(Some(0));
    host
}

/// Replaces the scripted child with a single exit code.
fn exit_with(host: &mut FakeHost, code: i32) {
    host.child_script.clear();
    host.child_script.push_back(Some(code));
}

/// Counts calls of one kind.
fn count(host: &FakeHost, predicate: impl Fn(&Call) -> bool) -> usize {
    host.count(predicate)
}

/// The expected fresh IW3 array.
fn expected_fresh() -> Vec<std::ffi::OsString> {
    vec![
        SETUP_PATH.into(),
        "--installto".into(),
        ROOT.into(),
        "--silent".into(),
        "--log".into(),
        LOG.into(),
        "--verbose".into(),
    ]
}

/// The expected upgrade IW3 array, written out literally.
fn expected_upgrade(payload: &Payload) -> Vec<std::ffi::OsString> {
    let root = PathBuf::from(UPGRADE_ROOT);
    vec![
        root.join("Update.exe").into_os_string(),
        "apply".into(),
        "--package".into(),
        root.join("packages")
            .join(&payload.package_name)
            .into_os_string(),
        "--silent".into(),
        "--norestart".into(),
        "--root".into(),
        root.clone().into_os_string(),
        "--packageDir".into(),
        root.join("packages").into_os_string(),
        "--log".into(),
        LOG.into(),
        "--verbose".into(),
    ]
}

/// Successful fresh and upgrade runs, nonzero exits and the 180 s progress text.
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one test covers the success, failure and deadline cases from the plan bullet"
)]
fn success_fresh_and_upgrade_exit_codes_and_progress() {
    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());
    let mut host = fresh_host();
    let (result, _) = run(&mut host, fresh_request(), &payload);
    let Ok(Installed {
        completion,
        legacy_marker_failed,
    }) = result
    else {
        panic!("fresh install failed");
    };
    assert_eq!(completion, Completion::Installed);
    assert!(!legacy_marker_failed);
    let written = host.calls.iter().find_map(|call| match call {
        Call::WriteMarker(marker) => Some(marker),
        _ => None,
    });
    let marker = written.expect("fresh marker written");
    assert_eq!(marker.pack_id, "NavoApp");
    assert_eq!(marker.install_id.len(), 32);
    assert_eq!(marker.created_by, format!("navo-setup {VERSION}"));
    assert!(host.calls.iter().any(|call| matches!(
        call,
        Call::Cleanup {
            remove_created: false
        }
    )));
    assert_eq!(
        count(&host, |call| matches!(call, Call::RemoveShortcuts { .. })),
        1
    );

    let mut host = upgrade_host("0.3.0", true);
    let (result, _) = run(&mut host, upgrade_request("0.3.0", true), &payload);
    assert!(matches!(result, Ok(Installed { .. })));
    assert_eq!(
        count(&host, |call| matches!(call, Call::RemoveShortcuts { .. })),
        0
    );

    let mut host = fresh_host();
    host.root_empty.clear();
    host.root_empty.push_back(Ok(true));
    exit_with(&mut host, 1);
    let (result, _) = run(&mut host, fresh_request(), &payload);
    let error = result.err().unwrap();
    assert!(matches!(error, InstallError::RetryEmpty(_, _)));
    assert_eq!(error.exit_code(), 5);

    let mut host = upgrade_host("0.3.0", true);
    exit_with(&mut host, 3);
    let (result, _) = run(&mut host, upgrade_request("0.3.0", true), &payload);
    let error = result.err().unwrap();
    assert!(matches!(
        error,
        InstallError::Failed(Failure::Upgrade(Some(3)))
    ));
    assert_eq!(error.exit_code(), 5);

    let mut host = fresh_host();
    host.child_script.clear();
    for _ in 0..180 {
        host.child_script.push_back(None);
    }
    host.child_script.push_back(Some(0));
    let (result, progress) = run(&mut host, fresh_request(), &payload);
    assert!(matches!(result, Ok(Installed { .. })));
    assert_eq!(progress.len(), 180);
    assert!(!progress[178].still_working);
    assert!(progress[179].still_working);
}

/// Both spawned arrays equal IW3's arrays written out, compared whole.
#[test]
fn the_spawned_arrays_equal_iw3s_for_both_modes() {
    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());
    let mut host = fresh_host();
    let _ = run(&mut host, fresh_request(), &payload);
    assert_eq!(host.spawns(), vec![&expected_fresh()]);

    let mut host = upgrade_host("0.3.0", true);
    let _ = run(&mut host, upgrade_request("0.3.0", true), &payload);
    assert_eq!(host.spawns(), vec![&expected_upgrade(&payload)]);
}

/// A change before spawn refuses without extracting or spawning; after exit it fails with code 5.
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one test covers the before-spawn, after-exit and identity-mismatch cases"
)]
fn a_changed_ancestor_identity_or_held_folder_fails_closed() {
    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());

    let mut host = fresh_host();
    host.ancestors.push_back(false);
    let (result, _) = run(&mut host, fresh_request(), &payload);
    let error = result.err().unwrap();
    assert!(matches!(error, InstallError::Failed(Failure::Changed)));
    assert_eq!(error.exit_code(), 3);
    assert_eq!(count(&host, |call| matches!(call, Call::Extract)), 0);
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

    let mut host = fresh_host();
    host.ancestors.extend([true, true, false]);
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

    let mut host = upgrade_host("0.3.0", true);
    host.ancestors.push_back(false);
    let (result, _) = run(&mut host, upgrade_request("0.3.0", true), &payload);
    let error = result.err().unwrap();
    assert!(matches!(error, InstallError::Failed(Failure::Changed)));
    assert_eq!(error.exit_code(), 3);
    assert_eq!(count(&host, |call| matches!(call, Call::Stage)), 0);
    assert_eq!(count(&host, |call| matches!(call, Call::Spawn(_))), 0);

    let mut host = upgrade_host("0.3.0", true);
    host.pin_existing_ok = false;
    let (result, _) = run(&mut host, upgrade_request("0.3.0", true), &payload);
    assert!(matches!(
        result,
        Err(InstallError::Failed(Failure::Folder(
            FolderProblem::ProbeFailed
        )))
    ));
    assert_eq!(count(&host, |call| matches!(call, Call::Spawn(_))), 0);
}

#[path = "runner_tests_extra.rs"]
mod extra;
#[path = "runner_tests_more.rs"]
mod more;
