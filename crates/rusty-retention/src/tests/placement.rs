use rusty_fs::UnixAccess;

use super::*;
use crate::place::is_private;

fn try_capture(vault: &mut Vault<'_>, source: &Location) -> Result<RetentionEntry, Error> {
    run(|run| {
        vault.capture(
            &CaptureRequest {
                source,
                policy: RetentionPolicy::new(1, 1000, 0).unwrap(),
                created_at: 0,
                job: None,
            },
            run,
        )
    })
}

fn exists(fs: &FakeFs, path: &str) -> bool {
    fs.metadata(&fs.location(path)).is_ok()
}

fn with_fallback<'a>(profile: &Path, fs: &'a FakeFs) -> Vault<'a> {
    fs.add_folder("profile");
    let mut vault = open(profile, fs);
    vault.places.fallback = Some(fs.location("profile/vault"));
    vault
}

#[test]
fn a_private_vault_belongs_to_this_user_and_has_no_group_or_other_bits() {
    let access = |owner, mode| UnixAccess { owner, mode };
    assert!(is_private(access(7, 0o40700), Some(7)));
    assert!(!is_private(access(8, 0o40700), Some(7)));
    assert!(!is_private(access(7, 0o40770), Some(7)));
    assert!(!is_private(access(7, 0o40704), Some(7)));
    assert!(!is_private(access(7, 0o40700), None));
}

#[cfg(unix)]
#[test]
fn a_foreign_owned_vault_is_skipped_for_the_next_candidate() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/docs/a", 7);
    fs.add_folder(".navo-vault");
    let me = crate::place::user_id().unwrap();
    fs.set_access(
        ".navo-vault",
        UnixAccess {
            owner: me + 1,
            mode: 0o40700,
        },
    );
    let mut vault = with_fallback(profile.path(), &fs);
    let entry = capture(&mut vault, &fs.location("home/docs/a"));
    assert_eq!(entry.record.anchor(), Some(fs.location("profile/vault")));
    assert!(
        exists(&fs, ".navo-vault"),
        "a vault we did not create stays"
    );
}

#[cfg(unix)]
#[test]
fn a_vault_refused_after_we_created_it_is_removed_again() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/docs/a", 7);
    let me = crate::place::user_id().unwrap();
    fs.set_access(
        ".navo-vault",
        UnixAccess {
            owner: me + 1,
            mode: 0o40755,
        },
    );
    fs.set_access(
        &format!(".Trash-{me}"),
        UnixAccess {
            owner: me + 1,
            mode: 0o40777,
        },
    );
    let mut vault = with_fallback(profile.path(), &fs);
    fs.set_volume("profile", 42);
    let source = fs.location("home/docs/a");
    assert!(matches!(
        try_capture(&mut vault, &source),
        Err(Error::Unsupported(_))
    ));
    assert!(!exists(&fs, ".navo-vault"));
    assert!(!exists(&fs, &format!(".Trash-{me}")));
    assert_eq!(fs.metadata(&source).unwrap().size, 7);
}

#[cfg(unix)]
#[test]
fn a_world_readable_vault_of_ours_is_tightened_to_owner_only_and_used() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/docs/a", 7);
    fs.add_folder(".navo-vault");
    let me = crate::place::user_id().unwrap();
    let wide = UnixAccess {
        owner: me,
        mode: 0o40755,
    };
    fs.set_access(".navo-vault", wide);
    let mut vault = with_fallback(profile.path(), &fs);
    let entry = capture(&mut vault, &fs.location("home/docs/a"));
    let anchor = entry.record.anchor().unwrap().path().to_text();
    assert_eq!(anchor.trim_start_matches('/'), ".navo-vault");
    let tightened = fs.unix_access(&fs.location(".navo-vault")).unwrap();
    assert_eq!(tightened.unwrap().mode & 0o777, 0o700);
}

#[cfg(unix)]
#[test]
fn a_volume_that_keeps_no_permissions_still_skips_a_foreign_vault() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/docs/a", 7);
    fs.add_folder(".navo-vault");
    fs.fix_permissions();
    let me = crate::place::user_id().unwrap();
    fs.set_access(
        ".navo-vault",
        UnixAccess {
            owner: me + 1,
            mode: 0o40777,
        },
    );
    let mut vault = with_fallback(profile.path(), &fs);
    let entry = capture(&mut vault, &fs.location("home/docs/a"));
    assert_eq!(entry.record.anchor(), Some(fs.location("profile/vault")));
}

#[test]
fn a_capture_refused_while_reading_the_source_creates_no_vault() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/docs/a", 7);
    fs.fault(
        "home/docs",
        FakeOp::List,
        1,
        Fault::Fail(FsErrorKind::NetworkLost),
    );
    let mut vault = open(profile.path(), &fs);
    assert!(try_capture(&mut vault, &fs.location("home/docs")).is_err());
    assert!(!exists(&fs, ".navo-vault"));
    assert!(!exists(&fs, "home/.navo-vault"));
}

#[test]
fn a_cross_device_rename_moves_the_capture_to_the_next_vault() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/docs/a", 7);
    fs.fault(
        "home/docs/a",
        FakeOp::Rename,
        1,
        Fault::FailOs {
            kind: FsErrorKind::Unknown,
            code: crate::capture::CROSS_DEVICE,
        },
    );
    let mut vault = with_fallback(profile.path(), &fs);
    let entry = capture(&mut vault, &fs.location("home/docs/a"));
    assert_ne!(entry.record.anchor(), Some(fs.location(".navo-vault")));
    let first = vault.list_after(0, 10).unwrap().remove(0);
    assert_eq!(first.state, RetainedState::Aborted);
    assert!(fs.metadata(&first.record.container).is_err());
    restore(&mut vault, entry.id, &fs.location("home/docs/a")).unwrap();
    assert_eq!(fs.metadata(&fs.location("home/docs/a")).unwrap().size, 7);
}

#[test]
fn any_other_rename_failure_still_stops_the_capture() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/docs/a", 7);
    fs.fault(
        "home/docs/a",
        FakeOp::Rename,
        1,
        Fault::Fail(FsErrorKind::SharingViolation),
    );
    let mut vault = with_fallback(profile.path(), &fs);
    assert!(try_capture(&mut vault, &fs.location("home/docs/a")).is_err());
    assert_eq!(vault.list_after(0, 10).unwrap().len(), 1);
    assert_eq!(fs.metadata(&fs.location("home/docs/a")).unwrap().size, 7);
}

#[cfg(unix)]
mod local {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn mode(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn a_world_readable_vault_of_ours_is_tightened_and_new_vaults_are_private() {
        let profile = tempfile::tempdir().unwrap();
        let files = tempfile::tempdir().unwrap();
        let shared = files.path().join(".navo-vault");
        std::fs::create_dir(&shared).unwrap();
        std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(files.path().join("a"), b"x").unwrap();
        let fs = LocalFs::new();
        let mut vault = open_within(profile.path(), &fs, files.path());
        let entry = capture(&mut vault, &location(&files.path().join("a")));
        assert_eq!(entry.record.anchor(), Some(location(&shared)));
        assert_eq!(mode(&shared), 0o700);
        assert_eq!(mode(Path::new(entry.record.container.as_str())), 0o700);
        assert!(!profile.path().join("vault").exists());
    }

    #[test]
    fn a_vault_that_refuses_a_new_container_moves_the_capture_on() {
        if crate::place::user_id() == Some(0) {
            return;
        }
        let profile = tempfile::tempdir().unwrap();
        let files = tempfile::tempdir().unwrap();
        let locked = files.path().join(".navo-vault");
        std::fs::create_dir(&locked).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o500)).unwrap();
        std::fs::write(files.path().join("a"), b"x").unwrap();
        let fs = LocalFs::new();
        let mut vault = open_within(profile.path(), &fs, files.path());
        let entry = capture(&mut vault, &location(&files.path().join("a")));
        assert_eq!(
            entry.record.anchor(),
            Some(location(&profile.path().join("vault")))
        );
        let first = vault.list_after(0, 10).unwrap().remove(0);
        assert_eq!(first.state, RetainedState::Aborted);
        assert_eq!(std::fs::read_dir(&locked).unwrap().count(), 0);
    }
}
