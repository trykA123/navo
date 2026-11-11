//! Fake-host silent-mode tests for IW17 exit codes.

use super::*;

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

/// Builds one valid marker.
fn marker() -> crate::marker::Marker {
    crate::marker::Marker {
        pack_id: "NavoApp".to_owned(),
        install_id: "0123456789abcdef0123456789abcdef".to_owned(),
        created_by: format!("navo-setup {VERSION}"),
    }
}

/// Builds one valid install at `root`.
fn install(root: &str, version: &str) -> Install {
    Install {
        root: PathBuf::from(root),
        identity: FileIdentity {
            volume: 1,
            file: [2; 16],
            kind: FileIdentityKind::Native,
        },
        safe_chain: true,
        update_exe: true,
        sq_version: Ok(sq(version)),
        marker: Some(Ok(marker())),
    }
}

/// Builds a discovery with one registry install.
fn discovery(version: &str) -> Discovery {
    Discovery {
        registry: Some(Ok(install(ROOT, version))),
        legacy: None,
    }
}

/// Runs silent mode with one fake host and returns its exit code.
fn run_silent(host: &mut FakeHost, payload: Option<Payload>) -> ExitCode {
    run_to(host, payload, ROOT)
}

/// Runs silent mode into one requested folder.
fn run_to(host: &mut FakeHost, payload: Option<Payload>, root: &str) -> ExitCode {
    run(
        &args(&["--silent", "--installto", root]),
        payload,
        Some(host),
        &test_logs,
    )
}

/// Returns the two log paths used by the fake runs.
fn test_logs() -> std::io::Result<LogPaths> {
    Ok(LogPaths {
        wizard: PathBuf::from("C:/Temp/NavoSetup-logs/test.log"),
        velopack: PathBuf::from(LOG),
    })
}

/// A successful fresh silent run returns 0.
#[test]
fn a_successful_silent_run_returns_0() {
    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());
    let mut host = FakeHost::new();
    host.discoveries
        .extend([Ok(Discovery::default()), Ok(Discovery::default())]);
    host.registry
        .extend([Registry::Absent, Registry::NamesRoot]);
    host.verified.push_back(Verified {
        update_exe: true,
        sq_version: Some(sq(VERSION)),
    });
    host.child_script.push_back(Some(0));
    assert_eq!(run_silent(&mut host, Some(payload)), ExitCode::SUCCESS);
}

/// Each reachable IW17 failure maps to its documented exit code.
#[test]
fn silent_failures_map_to_iw17_codes() {
    let dir = tempfile::tempdir().unwrap();

    let mut host = FakeHost::new();
    host.elevated.push_back(Ok(true));
    assert_eq!(
        run_silent(&mut host, Some(payload(dir.path()))),
        ExitCode::from(7)
    );

    let mut host = FakeHost::new();
    host.discoveries
        .extend([Ok(discovery(VERSION)), Ok(discovery(VERSION))]);
    assert_eq!(
        run_silent(&mut host, Some(payload(dir.path()))),
        ExitCode::from(8)
    );

    let mut host = FakeHost::new();
    host.discoveries
        .push_back(Err(std::io::Error::other("discovery failed")));
    assert_eq!(
        run_silent(&mut host, Some(payload(dir.path()))),
        ExitCode::from(6)
    );

    let mut host = FakeHost::new();
    host.discoveries
        .extend([Ok(Discovery::default()), Ok(discovery("0.3.0"))]);
    assert_eq!(
        run_silent(&mut host, Some(payload(dir.path()))),
        ExitCode::from(3)
    );

    let mut host = FakeHost::new();
    host.discoveries
        .extend([Ok(discovery("0.3.0")), Ok(discovery("0.3.0"))]);
    host.running.push_back(Ok(true));
    assert_eq!(
        run_silent(&mut host, Some(payload(dir.path()))),
        ExitCode::from(4)
    );

    let mut host = FakeHost::new();
    host.discoveries
        .extend([Ok(Discovery::default()), Ok(Discovery::default())]);
    host.registry.push_back(Registry::Absent);
    host.child_script.push_back(Some(1));
    assert_eq!(
        run_silent(&mut host, Some(payload(dir.path()))),
        ExitCode::from(5)
    );

    let mut host = FakeHost::new();
    assert_eq!(run_silent(&mut host, None), ExitCode::from(9));
}

/// Direct runner errors keep their IW17 codes in one table.
#[test]
fn runner_errors_keep_their_codes() {
    let cases = [
        (InstallError::Elevated, 7),
        (InstallError::Blocked, 4),
        (InstallError::NoPayload, 9),
        (InstallError::Extract, 5),
        (InstallError::WrongAction, 5),
        (InstallError::Failed(Failure::EmptyRoot(None)), 5),
        (InstallError::ModeChanged(Box::new(Mode::Fresh)), 3),
    ];
    for (error, code) in cases {
        assert_eq!(error.exit_code(), code);
    }
}

/// Runs silent mode into `root` with both logs in `dir`; returns the code and the wizard log.
fn run_logged(host: &mut FakeHost, root: &str, dir: &Path) -> (ExitCode, String, PathBuf) {
    let payload_dir = tempfile::tempdir().unwrap();
    let wizard = dir.join("run.log");
    let velopack = dir.join("run-velopack.log");
    let paths = LogPaths {
        wizard: wizard.clone(),
        velopack,
    };
    let logs = move || Ok(paths.clone());
    let args = args(&["--silent", "--installto", root]);
    let code = run(&args, Some(payload(payload_dir.path())), Some(host), &logs);
    (
        code,
        fs::read_to_string(&wizard).unwrap_or_default(),
        wizard,
    )
}

/// Builds one fake host per failing IW17 exit 3 to 8, with the folder each one asks for.
fn failing_hosts() -> Vec<(u8, &'static str, FakeHost)> {
    let host = |setup: &dyn Fn(&mut FakeHost)| {
        let mut host = FakeHost::new();
        setup(&mut host);
        host
    };
    vec![
        (
            3,
            r"C:\Elsewhere\Navo",
            host(&|h| h.discoveries.extend([Ok(discovery("0.3.0"))])),
        ),
        (
            3,
            ROOT,
            host(&|h| {
                h.discoveries
                    .extend([Ok(Discovery::default()), Ok(discovery("0.3.0"))])
            }),
        ),
        (
            4,
            ROOT,
            host(&|h| {
                h.discoveries
                    .extend([Ok(discovery("0.3.0")), Ok(discovery("0.3.0"))]);
                h.running.push_back(Ok(true));
            }),
        ),
        (
            5,
            ROOT,
            host(&|h| {
                h.discoveries
                    .extend([Ok(Discovery::default()), Ok(Discovery::default())]);
                h.registry.push_back(Registry::Absent);
                h.child_script.push_back(Some(1));
            }),
        ),
        (
            6,
            ROOT,
            host(&|h| {
                h.discoveries
                    .push_back(Err(std::io::Error::other("registry")))
            }),
        ),
        (7, ROOT, host(&|h| h.elevated.push_back(Ok(true)))),
        (
            8,
            ROOT,
            host(&|h| {
                h.discoveries
                    .extend([Ok(discovery(VERSION)), Ok(discovery(VERSION))])
            }),
        ),
    ]
}

/// Every failing exit prints the wizard log's path, and the log holds the same lines (S10, IW17).
#[test]
fn every_failing_silent_exit_names_its_log() {
    for (code, root, mut host) in failing_hosts() {
        let dir = tempfile::tempdir().unwrap();
        let (exit, text, wizard) = run_logged(&mut host, root, dir.path());
        assert_eq!(exit, ExitCode::from(code), "{text}");
        let named = format!("navo-setup: log: {}", wizard.display());
        assert!(
            text.lines().any(|line| line == named),
            "exit {code}: {text:?}"
        );
        assert!(
            text.lines().count() >= 2,
            "exit {code} logs its message too: {text:?}"
        );
    }
}

/// An unsafe folder name gets IW8's own advice in silent mode too, never "uninstall".
#[test]
fn an_unsafe_name_is_never_told_to_uninstall() {
    let why = crate::mode::Why::UnsafeName {
        root: PathBuf::from(r"C:\R&D\Navo"),
        character: '&',
    };
    let error = InstallError::ModeChanged(Box::new(Mode::Ambiguous(why)));
    let message = crate::cli::messages::install_message(&error, Path::new(ROOT), false);
    assert_eq!(
        message,
        "Navo is installed in `C:\\R&D\\Navo`. Setup cannot update a folder whose name contains `&`. Keep using Navo there; it updates itself."
    );
}

/// A different --installto while an install exists exits 3.
#[test]
fn a_different_installto_while_installed_exits_3() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = FakeHost::new();
    host.discoveries
        .extend([Ok(discovery("0.3.0")), Ok(discovery("0.3.0"))]);
    assert_eq!(
        run_to(&mut host, Some(payload(dir.path())), r"C:\Elsewhere\Navo"),
        ExitCode::from(3)
    );
}

/// A differently cased --installto names the installed folder and proceeds.
#[test]
fn a_differently_cased_installto_proceeds() {
    #[cfg(windows)]
    let lower = r"c:/programs/navo";
    #[cfg(not(windows))]
    let lower = "/programs/navo";
    let dir = tempfile::tempdir().unwrap();
    let mut host = FakeHost::new();
    host.discoveries
        .extend([Ok(discovery("0.3.0")), Ok(discovery("0.3.0"))]);
    host.running.extend([Ok(false), Ok(false)]);
    host.registry.push_back(Registry::NamesRoot);
    host.markers.push_back(Some(marker()));
    host.verified.push_back(Verified {
        update_exe: true,
        sq_version: Some(sq(VERSION)),
    });
    host.child_script.push_back(Some(0));
    assert_eq!(
        run_to(&mut host, Some(payload(dir.path())), lower),
        ExitCode::SUCCESS
    );
}

#[path = "cli_recovery_tests.rs"]
mod recovery;

/// The child exit code reaches the IW17 exit-5 stderr line.
#[test]
fn the_child_exit_code_reaches_the_failure_message() {
    use crate::flow::{InstallAction, Request, Shortcuts};

    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());
    let request = Request {
        mode: Box::new(Mode::Fresh),
        root: PathBuf::from(ROOT),
        shortcuts: Shortcuts {
            desktop: true,
            start_menu: true,
        },
        action: InstallAction::Run,
    };
    let mut host = FakeHost::new();
    host.discoveries.push_back(Ok(Discovery::default()));
    host.registry.push_back(Registry::Absent);
    host.child_script.push_back(Some(1603));
    let error = crate::runner::install(request, &payload, Path::new(LOG), &mut host, &mut |_| {})
        .expect_err("child must fail");
    assert!(matches!(
        error,
        InstallError::Failed(Failure::PartialRoot(Some(1603)))
    ));
    let message = crate::cli::messages::install_message(&error, Path::new(ROOT), false);
    assert!(message.contains("1603"), "{message}");
    assert!(message.contains(ROOT), "{message}");
    assert!(message.contains("Delete this folder"), "{message}");
}
