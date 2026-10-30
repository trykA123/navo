use std::cell::Cell;
use std::ffi::OsString;
use std::process::ExitCode;

use rusty_store::StoreDirs;

#[test]
fn crash_startup_without_an_instance_guard_leaves_no_marker() {
    let root = tempfile::tempdir().unwrap();
    let dirs = StoreDirs::in_profile(root.path());
    let profile = crate::profile::Profile::User(Some(dirs.clone()));
    let (pending, running) = super::prepare_crash(&profile, None);
    assert!(pending.is_none());
    assert!(running.is_none());
    assert!(!dirs.data.join("running").exists());
}

#[test]
fn crash_startup_without_an_instance_guard_ignores_the_previous_marker() {
    let root = tempfile::tempdir().unwrap();
    let dirs = StoreDirs::in_profile(root.path());
    let _previous = crate::crash::Running::start(&dirs.data).unwrap();
    let marker = dirs.data.join("running");
    let before = std::fs::read(&marker).unwrap();
    let profile = crate::profile::Profile::User(Some(dirs));
    let (pending, running) = super::prepare_crash(&profile, None);
    assert!(pending.is_none());
    assert!(running.is_none());
    assert_eq!(std::fs::read(marker).unwrap(), before);
}

#[test]
fn a_refused_command_never_detects_or_writes_user_folders() {
    let root = tempfile::tempdir().unwrap();
    let dirs = StoreDirs::in_profile(root.path());
    let detected = Cell::new(false);
    let status = super::execute([OsString::from("--metrics")], || {
        detected.set(true);
        Some(dirs.clone())
    });
    assert_eq!(status, ExitCode::from(2));
    assert!(!detected.get(), "a refused command detected user folders");
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    let missing = root.path().join("missing");
    let status = super::execute([OsString::from("--left"), missing.into_os_string()], || {
        panic!("a refused pane detected user folders")
    });
    assert_eq!(status, ExitCode::from(2));
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn a_missing_pane_folder_fails_before_opening_a_window() {
    let root = tempfile::tempdir().unwrap();
    let report = root.path().join("metrics.json");
    let missing = root.path().join("missing");
    let args = [
        OsString::from("--metrics"),
        report.as_os_str().to_owned(),
        OsString::from("--left"),
        missing.as_os_str().to_owned(),
    ];
    let status = super::execute(args, || panic!("metrics detected user folders"));
    assert_eq!(status, ExitCode::from(2));
    assert!(!report.exists());
}

#[test]
fn a_user_profile_migrates_even_without_an_instance_guard() {
    let root = tempfile::tempdir().unwrap();
    let dirs = StoreDirs::in_profile(root.path());
    let user = crate::profile::Profile::User(Some(dirs.clone()));
    let folder = crate::profile::Profile::Folder(dirs);
    let start = crate::cli::Start::default();
    let metrics = crate::cli::Start {
        metrics: Some(root.path().join("metrics.json")),
        ..crate::cli::Start::default()
    };
    assert!(super::migrates(&start, &user, false));
    assert!(super::migrates(&start, &user, true));
    assert!(super::migrates(&start, &folder, true));
    assert!(!super::migrates(&start, &folder, false));
    assert!(!super::migrates(&metrics, &user, true));
    assert!(!super::migrates(
        &start,
        &crate::profile::Profile::User(None),
        false
    ));
}

#[test]
fn preparing_the_instance_creates_no_profile_folder_before_the_migration() {
    let root = tempfile::tempdir().unwrap();
    let dirs = StoreDirs {
        config: root.path().join("roaming/Navo"),
        data: root.path().join("local/Navo"),
    };
    let profile = crate::profile::Profile::User(Some(dirs.clone()));
    let (sender, _) = rusty_ui::window_view::open_requests();
    let prepared =
        crate::instance_start::prepare_instance(&crate::cli::Start::default(), &profile, sender);
    assert!(matches!(prepared, Ok(std::ops::ControlFlow::Continue(_))));
    assert!(!dirs.config.exists() && !dirs.data.exists());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn a_second_instance_in_another_retention_mode_is_refused_with_a_message() {
    for running in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let profile = crate::profile::Profile::Folder(StoreDirs::in_profile(root.path()));
        let first = crate::cli::Start {
            retention: running,
            ..Default::default()
        };
        let (sender, _requests) = rusty_ui::window_view::open_requests();
        let Ok(std::ops::ControlFlow::Continue((guard, mut splash))) =
            crate::instance_start::prepare_instance(&first, &profile, sender)
        else {
            panic!("expected the first instance");
        };
        assert!(guard.is_some());
        splash.close();
        for retention in [!running, running] {
            let start = crate::cli::Start {
                retention,
                ..Default::default()
            };
            let shown = Cell::new(None);
            let (sender, _) = rusty_ui::window_view::open_requests();
            let reply = crate::instance_start::prepare_instance_with_notice(
                &start,
                &profile,
                sender,
                |message| {
                    shown.set(Some(message.to_owned()));
                },
            )
            .unwrap();
            let std::ops::ControlFlow::Break(code) = reply else {
                panic!("a second instance must exit");
            };
            let refused = retention != running;
            let expected = if refused {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            };
            assert_eq!(code, expected, "running {running}, request {retention}");
            assert_eq!(
                shown.take(),
                refused.then(|| rusty_l10n::tr("instance-mode-mismatch"))
            );
        }
    }
}

#[test]
fn the_store_check_follows_the_retention_mode() {
    let root = tempfile::tempdir().unwrap();
    let dirs = StoreDirs::in_profile(root.path());
    std::fs::write(&dirs.data, b"not a folder").unwrap();
    let profile = crate::profile::Profile::Folder(dirs);
    for retention in [true, false] {
        let start = crate::cli::Start {
            retention,
            ..Default::default()
        };
        let store = super::check_store(&profile, false, &start);
        assert_eq!(store.notice.is_some(), retention, "retention {retention}");
    }
}
