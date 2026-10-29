use rusty_core::retention::VAULT_RECORD_VERSION;

use super::*;
use crate::place::{Places, candidates, volume_root};

#[cfg(unix)]
fn try_capture(vault: &mut Vault<'_>, source: &Location) -> Result<RetentionEntry, Error> {
    run(|run| {
        vault.capture(
            CaptureRequest {
                source,
                policy: RetentionPolicy::new(1, 1000, 0).unwrap(),
                created_at: 0,
                job: None,
            },
            run,
        )
    })
}

fn names(fs: &FakeFs, folder: &str) -> Vec<String> {
    let mut found = Vec::new();
    fs.list(
        &fs.location(folder),
        &mut |batch: rusty_fs::EntryBatch| {
            found.extend(batch.entries.iter().map(|entry| entry.name.to_text()));
            std::ops::ControlFlow::Continue(())
        },
        &CancelToken::new(),
    )
    .unwrap();
    found
}

fn vault_of(entry: &RetentionEntry) -> String {
    entry
        .record
        .anchor()
        .unwrap()
        .path()
        .to_text()
        .trim_start_matches('/')
        .to_owned()
}

#[cfg(unix)]
fn deny_create(fs: &FakeFs, path: &str) {
    fs.fault(
        path,
        FakeOp::Create,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
}

#[test]
fn a_capture_lands_in_the_volume_root_vault_and_leaves_the_folder_clean() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("mnt/usb/docs/a", 7);
    fs.add_file("mnt/usb/docs/b", 3);
    fs.set_volume("mnt/usb", 9);
    let mut vault = open(profile.path(), &fs);
    let entry = capture(&mut vault, &fs.location("mnt/usb/docs/a"));
    assert_eq!(entry.record.version, VAULT_RECORD_VERSION);
    assert_eq!(vault_of(&entry), "mnt/usb/.navo-vault");
    assert_eq!(names(&fs, "mnt/usb/docs"), ["b"]);
    assert_eq!(names(&fs, "mnt/usb/.navo-vault").len(), 1);
    restore(&mut vault, entry.id, &fs.location("mnt/usb/docs/a")).unwrap();
    assert_eq!(fs.metadata(&fs.location("mnt/usb/docs/a")).unwrap().size, 7);
    assert!(names(&fs, "mnt/usb/.navo-vault").is_empty());
}

#[test]
fn expiry_removes_the_container_from_the_vault_but_keeps_the_vault() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/docs/a", 7);
    fs.set_available_space("home/docs", Some(u64::MAX));
    let mut vault = open(profile.path(), &fs);
    let entry = capture(&mut vault, &fs.location("home/docs/a"));
    let at = fs.location("home/docs");
    let report = run(|run| {
        vault.maintain_volume(
            MaintenanceRequest {
                at: &at,
                policy: RetentionPolicy::default(),
                now: 4 * DAY_MS,
                max_purges: 8,
            },
            run,
        )
    })
    .unwrap();
    assert_eq!(report.purged, [entry.id]);
    assert!(fs.metadata(&entry.record.container).is_err());
    assert!(names(&fs, &vault_of(&entry)).is_empty());
}

#[test]
fn a_vault_entry_purges_after_its_folder_is_gone_and_restores_only_once_it_is_back() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/docs/a", 7);
    fs.add_file("home/docs/b", 3);
    let mut vault = open(profile.path(), &fs);
    let kept = capture(&mut vault, &fs.location("home/docs/a"));
    let gone = capture(&mut vault, &fs.location("home/docs/b"));
    fs.rename_entry("home/docs", "home/moved");
    assert!(restore(&mut vault, kept.id, &fs.location("home/docs/a")).is_err());
    assert!(fs.metadata(&kept.record.held().unwrap()).is_ok());
    assert_eq!(
        purge(&mut vault, gone.id).unwrap().state,
        RetainedState::Purged
    );
    assert!(fs.metadata(&gone.record.container).is_err());
    fs.rename_entry("home/moved", "home/docs");
    restore(&mut vault, kept.id, &fs.location("home/docs/a")).unwrap();
    assert_eq!(fs.metadata(&fs.location("home/docs/a")).unwrap().size, 7);
}

#[cfg(unix)]
#[test]
fn a_read_only_root_falls_back_to_the_profile_vault_on_the_same_device() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/docs/a", 7);
    fs.add_folder("profile");
    deny_create(&fs, ".navo-vault");
    let mut vault = open(profile.path(), &fs);
    vault.places.fallback = Some(fs.location("profile/vault"));
    let entry = capture(&mut vault, &fs.location("home/docs/a"));
    assert_eq!(vault_of(&entry), "profile/vault");
    assert!(names(&fs, "home/docs").is_empty());
    restore(&mut vault, entry.id, &fs.location("home/docs/a")).unwrap();
}

#[cfg(unix)]
#[test]
fn a_profile_vault_on_another_device_is_skipped_for_the_mount_trash() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/docs/a", 7);
    fs.add_folder("profile");
    fs.set_volume("profile", 42);
    deny_create(&fs, ".navo-vault");
    let mut vault = open(profile.path(), &fs);
    vault.places.fallback = Some(fs.location("profile/vault"));
    let entry = capture(&mut vault, &fs.location("home/docs/a"));
    let trash = format!(".Trash-{}/navo-vault", crate::place::user_id().unwrap());
    assert_eq!(vault_of(&entry), trash);
    assert!(names(&fs, "profile").is_empty());
}

#[cfg(unix)]
#[test]
fn no_same_device_vault_refuses_to_retain_and_leaves_the_file() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/docs/a", 7);
    fs.add_folder("profile");
    fs.set_volume("profile", 42);
    deny_create(&fs, ".navo-vault");
    deny_create(&fs, &format!(".Trash-{}", crate::place::user_id().unwrap()));
    let mut vault = open(profile.path(), &fs);
    vault.places.fallback = Some(fs.location("profile/vault"));
    let source = fs.location("home/docs/a");
    assert!(matches!(
        try_capture(&mut vault, &source),
        Err(Error::Unsupported(_))
    ));
    assert_eq!(fs.metadata(&source).unwrap().size, 7);
    assert!(names(&fs, "profile").is_empty());
    assert!(vault.list_after(0, 10).unwrap().is_empty());
}

#[test]
fn an_old_sibling_container_still_restores() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/docs/a", 7);
    let mut vault = open(profile.path(), &fs);
    let entry = capture(&mut vault, &fs.location("home/docs/a"));
    let name = entry.record.container.name().unwrap().to_text();
    fs.rename_entry(
        &entry.record.container.path().to_text(),
        &format!("home/docs/{name}"),
    );
    let mut record = entry.record.clone();
    record.version = 1;
    record.container = fs.location(&format!("home/docs/{name}"));
    record.parent_identity = fs
        .file_identity(&fs.location("home/docs"))
        .unwrap()
        .unwrap();
    record.validate().unwrap();
    let (db, _) = Db::open(&profile.path().join("ferrite.db"), false).unwrap();
    db.connection()
        .execute(
            "UPDATE retained_entry SET record = ?2 WHERE id = ?1",
            (entry.id.0, serde_json::to_vec(&record).unwrap()),
        )
        .unwrap();
    restore(&mut vault, entry.id, &fs.location("home/docs/a")).unwrap();
    assert_eq!(names(&fs, "home/docs"), ["a"]);
}

#[test]
fn one_stuck_entry_does_not_stop_expiry_of_the_others() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/a", 7);
    fs.add_file("home/b", 3);
    fs.set_available_space("home", Some(u64::MAX));
    let mut vault = open(profile.path(), &fs);
    let stuck = capture(&mut vault, &fs.location("home/a"));
    let other = capture(&mut vault, &fs.location("home/b"));
    let container = stuck.record.container.path().to_text();
    let seen = fs.calls(&container, FakeOp::Identity);
    for call in 1..=64 {
        fs.fault(
            &container,
            FakeOp::Identity,
            seen + call,
            Fault::Fail(FsErrorKind::NetworkLost),
        );
    }
    let at = fs.location("home");
    let report = run(|run| {
        vault.maintain_volume(
            MaintenanceRequest {
                at: &at,
                policy: RetentionPolicy::default(),
                now: 4 * DAY_MS,
                max_purges: 8,
            },
            run,
        )
    })
    .unwrap();
    assert_eq!(report.purged, [other.id]);
    assert_ne!(vault.get(stuck.id).unwrap().state, RetainedState::Purged);
    assert!(fs.metadata(&stuck.record.held().unwrap()).is_ok());
}

#[test]
fn the_volume_root_is_the_highest_folder_on_the_same_volume() {
    let fs = FakeFs::new();
    let start = fs.location("mnt/usb/docs/deep");
    let root = volume_root(&start, |at| at.as_str().contains("usb"));
    assert_eq!(root.path().to_text(), "mnt/usb");
}

#[test]
fn vault_candidates_follow_the_platform_order() {
    let fs = FakeFs::new();
    let root = fs.location("mnt");
    let parent = fs.location("mnt/a/b");
    let fallback = fs.location("profile/vault");
    let places = Places {
        root: &root,
        parent: &parent,
        fallback: Some(&fallback),
        user: Some(1000),
    };
    let text = |chains: Vec<Vec<Location>>| -> Vec<Vec<String>> {
        chains
            .into_iter()
            .map(|chain| chain.iter().map(|at| at.path().to_text()).collect())
            .collect()
    };
    assert_eq!(
        text(candidates(&places, false)),
        [
            vec!["mnt/.navo-vault"],
            vec!["profile/vault"],
            vec!["mnt/.Trash-1000", "mnt/.Trash-1000/navo-vault"],
        ]
    );
    assert_eq!(
        text(candidates(&places, true)),
        [
            vec!["mnt/.navo-vault"],
            vec!["mnt/a/.navo-vault"],
            vec!["mnt/a/b/.navo-vault"],
        ]
    );
}
