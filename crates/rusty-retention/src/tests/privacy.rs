#[cfg(unix)]
use rusty_fs::UnixAccess;

use super::*;

fn fallback_vault<'a>(profile: &Path, fs: &'a FakeFs) -> Vault<'a> {
    fs.add_folder("profile");
    let mut vault = open(profile, fs);
    vault.places.fallback = Some(fs.location("profile/vault"));
    vault
}

fn anchor(fs: &FakeFs, profile: &Path) -> String {
    let mut vault = fallback_vault(profile, fs);
    let entry = capture(&mut vault, &fs.location("home/docs/a"));
    let anchor = entry.record.anchor().unwrap().path().to_text();
    anchor.trim_start_matches('/').to_owned()
}

#[cfg(unix)]
fn shared_vault_of_ours(fs: &FakeFs) {
    fs.add_file("home/docs/a", 7);
    fs.add_folder(".navo-vault");
    fs.set_access(
        ".navo-vault",
        UnixAccess {
            owner: crate::place::user_id().unwrap(),
            mode: 0o40755,
        },
    );
}

#[cfg(unix)]
#[test]
fn a_vault_that_cannot_be_made_private_on_a_unix_filesystem_is_skipped() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    shared_vault_of_ours(&fs);
    fs.fault(
        ".navo-vault",
        FakeOp::Secure,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    assert_eq!(anchor(&fs, profile.path()), "profile/vault");
}

#[cfg(unix)]
#[test]
fn a_vault_whose_mode_stays_open_on_a_unix_filesystem_is_skipped() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    shared_vault_of_ours(&fs);
    fs.fix_permissions();
    assert_eq!(anchor(&fs, profile.path()), "profile/vault");
}

#[test]
fn a_foreign_vault_is_skipped_without_changing_it() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/docs/a", 7);
    fs.add_folder(".navo-vault");
    fs.set_owned_by_current_user(".navo-vault", false);
    assert_ne!(anchor(&fs, profile.path()), ".navo-vault");
    assert_eq!(fs.calls(".navo-vault", FakeOp::Secure), 0);
    assert!(fs.metadata(&fs.location(".navo-vault")).is_ok());
}

#[test]
fn a_vault_of_ours_is_secured_and_used() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/docs/a", 7);
    fs.add_folder(".navo-vault");
    fs.set_owned_by_current_user(".navo-vault", true);
    assert_eq!(anchor(&fs, profile.path()), ".navo-vault");
    assert!(fs.calls(".navo-vault", FakeOp::Secure) > 0);
}

#[test]
fn a_vault_created_now_is_trusted_even_when_a_share_reports_another_owner() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/docs/a", 7);
    fs.set_owned_by_current_user(".navo-vault", false);
    assert_eq!(anchor(&fs, profile.path()), ".navo-vault");
}
