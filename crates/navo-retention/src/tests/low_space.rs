//! Low free space (AD14): a full drive keeps no new deleted files, and cleanup spares items
//! kept in the last 24 hours.

use super::*;
use crate::KeepCheck;

fn policy() -> RetentionPolicy {
    RetentionPolicy::new(3, 1000, 10).unwrap()
}

#[test]
fn a_delete_on_a_full_drive_is_not_kept() {
    let fs = FakeFs::new();
    fs.add_file("home/a", 7);
    fs.set_available_space("home", Some(9));
    let check = crate::check_keep(&fs, &fs.location("home/a"), policy());
    assert_eq!(check.unwrap(), KeepCheck::DriveFull);
}

#[test]
fn a_drive_with_room_or_unknown_space_keeps_deletes_as_before() {
    let fs = FakeFs::new();
    fs.add_file("home/a", 7);
    let source = fs.location("home/a");
    assert_eq!(
        crate::check_keep(&fs, &source, policy()).unwrap(),
        KeepCheck::Keep
    );
    fs.set_available_space("home", Some(10));
    assert_eq!(
        crate::check_keep(&fs, &source, policy()).unwrap(),
        KeepCheck::Keep
    );
}

#[test]
fn a_drive_that_cannot_keep_or_a_missing_source_is_unsupported() {
    let fs = FakeFs::new();
    fs.add_file("home/a", 7);
    fs.set_available_space("home", Some(0));
    let missing = fs.location("home/missing");
    assert_eq!(
        crate::check_keep(&fs, &missing, policy()).unwrap(),
        KeepCheck::Unsupported
    );
    fs.set_volume_serial("home", None);
    let source = fs.location("home/a");
    assert_eq!(
        crate::check_keep(&fs, &source, policy()).unwrap(),
        KeepCheck::Unsupported
    );
}

#[test]
fn a_failed_space_query_is_an_error_not_a_keep() {
    let fs = FakeFs::new();
    fs.add_file("home/a", 7);
    fs.fault(
        "home",
        FakeOp::Space,
        1,
        Fault::Fail(FsErrorKind::NetworkLost),
    );
    let error = crate::check_keep(&fs, &fs.location("home/a"), policy()).unwrap_err();
    assert_eq!(error.kind, FsErrorKind::NetworkLost);
}

#[test]
fn low_space_cleanup_keeps_an_item_deleted_in_the_last_24_hours() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.set_available_space("home", Some(0));
    let mut vault = open(profile.path(), &fs);
    fs.add_file("home/a", 7);
    let kept = capture(&mut vault, &fs.location("home/a"));
    let at = fs.location("home");
    let request = MaintenanceRequest {
        at: &at,
        now: kept.record.created_at + DAY_MS - 1,
        policy: policy(),
        max_purges: 256,
    };
    let result = run(|run| vault.maintain_volume(&request, run)).unwrap();
    assert!(result.purged.is_empty());
    assert!(result.low_space);
    assert_eq!(vault.get(kept.id).unwrap().state, RetainedState::Ready);
    assert!(fs.metadata(&kept.record.held().unwrap()).is_ok());
}

/// Two files on two drives; `home` has room, `work` is set up by the caller.
fn two_drives() -> (FakeFs, [Location; 2]) {
    let fs = FakeFs::new();
    fs.add_file("home/a", 7);
    fs.add_file("work/b", 7);
    let sources = [fs.location("home/a"), fs.location("work/b")];
    (fs, sources)
}

#[test]
fn a_selection_with_one_source_on_a_full_drive_is_not_kept() {
    let (fs, sources) = two_drives();
    fs.set_available_space("work", Some(0));
    let check = crate::check_keep_all(&fs, &sources, policy());
    assert_eq!(check.unwrap(), KeepCheck::DriveFull);
}

#[test]
fn a_selection_with_one_source_on_a_drive_that_cannot_keep_is_unsupported() {
    let (fs, sources) = two_drives();
    fs.set_volume_serial("work", None);
    let check = crate::check_keep_all(&fs, &sources, policy());
    assert_eq!(check.unwrap(), KeepCheck::Unsupported);
}

#[test]
fn a_selection_measures_each_folder_once_and_keeps_when_all_have_room() {
    let fs = FakeFs::new();
    fs.add_file("home/a", 7);
    fs.add_file("home/b", 7);
    let sources = [fs.location("home/a"), fs.location("home/b")];
    let check = crate::check_keep_all(&fs, &sources, policy());
    assert_eq!(check.unwrap(), KeepCheck::Keep);
    assert_eq!(fs.calls("home", FakeOp::Space), 1);
}

#[test]
fn a_selection_passes_over_a_vanished_item_like_the_job_probe() {
    let fs = FakeFs::new();
    fs.add_file("home/a", 7);
    let sources = [fs.location("work/gone"), fs.location("home/a")];
    let check = crate::check_keep_all(&fs, &sources, policy());
    assert_eq!(check.unwrap(), KeepCheck::Keep);
}
