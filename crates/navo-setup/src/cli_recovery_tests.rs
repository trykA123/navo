//! Silent-mode tests for IW7 recovery text, shortcut notes and Same/Newer refusal.

use super::*;
use crate::flow::{InstallAction, Request, Shortcuts};

/// Another --installto for Same or Newer still exits 3.
#[test]
fn same_and_newer_with_another_installto_exit_3() {
    #[cfg(windows)]
    let elsewhere = "C:/Elsewhere/Navo";
    #[cfg(not(windows))]
    let elsewhere = "/Elsewhere/Navo";
    for version in [VERSION, "0.5.0"] {
        let mut host = FakeHost::new();
        host.discoveries.push_back(Ok(discovery(version)));
        assert_eq!(
            run_to(
                &mut host,
                Some(payload(std::env::temp_dir().as_path())),
                elsewhere
            ),
            ExitCode::from(3)
        );
    }
}

/// An upgrade with a no-shortcut flag writes the IW11 note to the wizard log.
#[test]
fn an_upgrade_shortcut_flag_writes_the_wizard_note() {
    let dir = tempfile::tempdir().unwrap();
    let wizard = dir.path().join("wizard.log");
    let log = wizard.clone();
    let logs = move || {
        Ok(LogPaths {
            wizard: log.clone(),
            velopack: PathBuf::from(LOG),
        })
    };
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
    let code = run(
        &args(&["--silent", "--installto", ROOT, "--no-desktop"]),
        Some(payload(dir.path())),
        Some(&mut host),
        &logs,
    );
    assert_eq!(code, ExitCode::SUCCESS);
    let text = std::fs::read_to_string(wizard).unwrap();
    assert!(text.contains("keeps the existing shortcuts"), "{text}");
}

/// The partial-root message names the folder and the IW7 delete line.
#[test]
fn the_partial_root_message_names_the_folder_and_delete_line() {
    let error = InstallError::Failed(Failure::PartialRoot(Some(1603)));
    let message = crate::cli::messages::install_message(&error, Path::new(ROOT), false);
    assert!(message.contains(ROOT), "{message}");
    assert!(
        message.contains("Delete this folder before you run Setup again"),
        "{message}"
    );
}

/// The Unlisted message names the exact uninstall command.
#[test]
fn the_unlisted_message_names_the_uninstall_command() {
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
    host.registry.extend([Registry::Absent, Registry::Absent]);
    host.verified.push_back(Verified {
        update_exe: true,
        sq_version: Some(sq(VERSION)),
    });
    host.child_script.push_back(Some(0));
    let error = crate::runner::install(request, &payload, Path::new(LOG), &mut host, &mut |_| {})
        .expect_err("must be unlisted");
    let message = crate::cli::messages::install_message(&error, Path::new(ROOT), false);
    let expected = format!("\"{}\\Update.exe\" uninstall", ROOT);
    assert!(message.contains(&expected), "{message}");
}

/// Exit-5 text names Update.exe apply for an upgrade child.
#[test]
fn the_upgrade_child_is_named_update_exe_apply() {
    let error = InstallError::Failed(Failure::Upgrade(Some(1)));
    let message = crate::cli::messages::install_message(&error, Path::new(ROOT), true);
    assert!(
        message.contains("Update.exe apply exited with code 1"),
        "{message}"
    );
}

/// Bad arguments exit 2 before the log factory is called.
#[test]
fn bad_arguments_do_not_create_logs() {
    let called = std::cell::Cell::new(false);
    let logs = || {
        called.set(true);
        Ok(LogPaths {
            wizard: PathBuf::from("wizard.log"),
            velopack: PathBuf::from("velopack.log"),
        })
    };
    let mut host = FakeHost::new();
    let code = run(
        &args(&["--silent", "--bogus"]),
        None,
        Some(&mut host),
        &logs,
    );
    assert_eq!(code, ExitCode::from(2));
    assert!(!called.get());
}
