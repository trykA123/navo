use super::ServiceQuery;
use super::install_check::{Mismatch, is_mismatch};
use super::install_plan::{CopyPlan, LEGACY_PRODUCT, NAVO};
use super::upgrade::{UpgradeNotice, await_answer, carry_legacy_config, remove_both};
use std::time::Duration;
use std::{cell::Cell, io, path::Path};

fn query(legacy_present: bool) -> impl FnOnce() -> io::Result<ServiceQuery> {
    move || {
        Ok(ServiceQuery {
            running: None,
            version: None,
            legacy_present,
        })
    }
}

#[test]
fn remove_handles_both_names_even_when_the_first_fails() {
    let mut removed = Vec::new();
    let result = remove_both([NAVO, LEGACY_PRODUCT], |product| {
        removed.push(product);
        if product == NAVO {
            return Err(io::Error::other("busy"));
        }
        Ok(())
    });
    assert!(result.is_err());
    assert_eq!(removed, [NAVO, LEGACY_PRODUCT]);
    let mut removed = Vec::new();
    remove_both([NAVO, LEGACY_PRODUCT], |product| {
        removed.push(product);
        Ok(())
    })
    .unwrap();
    assert_eq!(removed, [NAVO, LEGACY_PRODUCT]);
}

#[test]
fn legacy_cleanup_deletes_only_legacy_images_in_the_legacy_folders() {
    let root = tempfile::tempdir().unwrap();
    let plan = CopyPlan::of(LEGACY_PRODUCT, root.path(), 3);
    assert_eq!(plan.directory, root.path().join("Ferrite").join("Index"));
    std::fs::create_dir_all(&plan.directory).unwrap();
    for name in ["ferrite.exe.2.old", "navo.exe.2.old", "ferrite.exe"] {
        std::fs::write(plan.directory.join(name), b"x").unwrap();
    }
    plan.delete_old().unwrap();
    assert!(!plan.directory.join("ferrite.exe.2.old").exists());
    assert!(plan.directory.join("navo.exe.2.old").exists());
    let [parent, index] = LEGACY_PRODUCT.data_directories(Path::new("program-data"));
    assert_eq!(parent, Path::new("program-data/Ferrite"));
    assert_eq!(index, Path::new("program-data/Ferrite/index"));
}

#[test]
fn waiting_for_the_new_pipe_is_bounded_and_ends_on_the_first_answer() {
    let calls = Cell::new(0);
    let limit = (Duration::from_millis(30), Duration::from_millis(5));
    let error = await_answer(limit, || {
        calls.set(calls.get() + 1);
        Err(io::ErrorKind::NotFound.into())
    })
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(!is_mismatch(&error), "a silent pipe is a timeout");
    assert!(calls.get() >= 2);
    calls.set(0);
    await_answer(limit, || {
        calls.set(calls.get() + 1);
        match calls.get() {
            1 => Err(io::Error::other(Mismatch::NotRunning)),
            _ => Ok(()),
        }
    })
    .unwrap();
    assert_eq!(calls.get(), 2);
}

#[test]
fn a_wrong_answer_until_the_deadline_is_a_mismatch() {
    let limit = (Duration::from_millis(20), Duration::from_millis(5));
    let tries = Cell::new(0);
    let error = await_answer(limit, || {
        tries.set(tries.get() + 1);
        match tries.get() {
            1 => Err(io::Error::other(Mismatch::NotRunning)),
            _ => Err(io::ErrorKind::NotFound.into()),
        }
    })
    .unwrap_err();
    assert!(is_mismatch(&error), "{error}");
    let error = await_answer(limit, || Err(io::Error::other(Mismatch::OldProcess))).unwrap_err();
    assert!(is_mismatch(&error), "{error}");
}

#[test]
fn the_notice_shows_while_legacy_index_exists_and_not_again_after_dismiss() {
    let root = tempfile::tempdir().unwrap();
    let notice = UpgradeNotice::new(Some(root.path().join("index-upgrade-dismissed")));
    assert!(!notice.is_due(query(false)));
    assert!(notice.is_due(query(true)));
    assert!(notice.is_due(query(true)));
    notice.dismiss().unwrap();
    assert!(!notice.is_due(query(true)));
    let again = UpgradeNotice::new(Some(root.path().join("index-upgrade-dismissed")));
    assert!(!again.is_due(query(true)));
}

#[test]
fn an_unreadable_scm_shows_no_notice_and_no_profile_dismisses_for_the_session_only() {
    let notice = UpgradeNotice::new(None);
    assert!(!notice.is_due(|| Err(io::ErrorKind::PermissionDenied.into())));
    notice.dismiss().unwrap();
    assert!(notice.is_due(query(true)));
}

#[test]
fn old_legacy_settings_are_carried_only_while_no_navo_settings_exist() {
    let missing: io::Result<&str> = Err(io::ErrorKind::NotFound.into());
    assert_eq!(
        carry_legacy_config(&missing, || Some("ferrite")),
        Some("ferrite")
    );
    let read = Cell::new(false);
    let legacy = || {
        read.set(true);
        Some("ferrite")
    };
    assert_eq!(carry_legacy_config(&Ok("navo"), legacy), None);
    let unreadable: io::Result<&str> = Err(io::ErrorKind::InvalidData.into());
    assert_eq!(carry_legacy_config(&unreadable, legacy), None);
    assert!(
        !read.get(),
        "the old service.json is never read over Navo settings"
    );
}
