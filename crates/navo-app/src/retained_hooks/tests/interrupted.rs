use navo_core::FsErrorKind;
use navo_fs::{FakeOp, Fault};
use navo_store::RetentionCatalog;

use super::*;

fn interrupt_next_restore(profile: &Profile, id: RetentionId, fault: Fault) {
    let (db, _) = crate::crash::open_db(&profile.dirs.journal_file()).unwrap();
    let entry = RetentionCatalog::new(db).unwrap().get(id).unwrap();
    let held = entry.record.held().unwrap().path().to_text();
    let next = profile.fs.calls(&held, FakeOp::Rename) + 1;
    profile.fs.fault(&held, FakeOp::Rename, next, fault);
}

fn deleted(profile: &Profile, fault: Fault) -> (Arc<dyn RetainedHooks>, RetentionId) {
    profile.fs.add_file("home/a.txt", 8);
    profile.delete(&["home/a.txt"]);
    let hooks = profile.hooks();
    let id = hooks.list(None, 10).unwrap()[0].id;
    interrupt_next_restore(profile, id, fault);
    assert!(hooks.restore(id).is_err());
    (hooks, id)
}

#[test]
fn an_interrupted_restore_lists_as_ready_and_restores_when_retried() {
    let profile = Profile::new();
    let (hooks, id) = deleted(&profile, Fault::Fail(FsErrorKind::Locked));
    assert_eq!(hooks.list(None, 10).unwrap()[0].state, RetainedState::Ready);
    assert_eq!(hooks.restore(id).unwrap(), profile.at("home/a.txt"));
    assert_eq!(
        profile.fs.metadata(&profile.at("home/a.txt")).unwrap().size,
        8
    );
    assert!(hooks.list(None, 10).unwrap().is_empty());
}

#[test]
fn a_retried_restore_that_had_already_landed_reports_where_it_landed() {
    let profile = Profile::new();
    let (hooks, id) = deleted(&profile, Fault::RenameThenFail(FsErrorKind::Locked));
    assert_eq!(hooks.restore(id).unwrap(), profile.at("home/a.txt"));
    assert_eq!(
        profile.fs.metadata(&profile.at("home/a.txt")).unwrap().size,
        8
    );
    assert!(hooks.list(None, 10).unwrap().is_empty());
}

#[test]
fn a_purge_of_an_interrupted_restore_removes_the_unchanged_payload() {
    let profile = Profile::new();
    let (hooks, id) = deleted(&profile, Fault::Fail(FsErrorKind::Locked));
    hooks.purge(id).unwrap();
    assert!(hooks.list(None, 10).unwrap().is_empty());
    assert!(!profile.exists("home/a.txt"));
}

fn held(profile: &Profile, id: RetentionId) -> Location {
    let (db, _) = crate::crash::open_db(&profile.dirs.journal_file()).unwrap();
    let entry = RetentionCatalog::new(db).unwrap().get(id).unwrap();
    entry.record.held().unwrap()
}

#[test]
fn an_interrupted_restore_whose_payload_changed_lists_as_needing_attention() {
    let profile = Profile::new();
    let (hooks, id) = deleted(&profile, Fault::Fail(FsErrorKind::Locked));
    let payload = held(&profile, id);
    profile.fs.add_file(&payload.path().to_text(), 9);
    assert!(hooks.purge(id).is_err());
    let rows = hooks.list(None, 10).unwrap();
    assert_eq!(rows[0].state, RetainedState::NeedsAttention);
    assert!(hooks.restore(id).is_err());
    assert_eq!(profile.fs.metadata(&payload).unwrap().size, 9);
}

#[test]
fn an_interrupted_restore_that_hits_a_transient_error_stays_ready() {
    let profile = Profile::new();
    let (hooks, id) = deleted(&profile, Fault::Fail(FsErrorKind::Locked));
    let payload = held(&profile, id).path().to_text();
    let next = profile.fs.calls(&payload, FakeOp::Identity) + 1;
    profile.fs.fault(
        &payload,
        FakeOp::Identity,
        next,
        Fault::Fail(FsErrorKind::NetworkLost),
    );
    assert!(hooks.purge(id).is_err());
    assert_eq!(hooks.list(None, 10).unwrap()[0].state, RetainedState::Ready);
    hooks.purge(id).unwrap();
    assert!(hooks.list(None, 10).unwrap().is_empty());
}

#[test]
fn a_purge_of_an_interrupted_restore_that_had_landed_says_so_and_keeps_the_file() {
    let profile = Profile::new();
    let (hooks, id) = deleted(&profile, Fault::RenameThenFail(FsErrorKind::Locked));
    let message = hooks.purge(id).unwrap_err();
    let home = profile.at("home");
    assert_eq!(
        message,
        navo_l10n::t!(
            "retained-already-restored",
            name = "a.txt",
            folder = home.as_str()
        )
    );
    assert_eq!(
        profile.fs.metadata(&profile.at("home/a.txt")).unwrap().size,
        8
    );
    assert!(hooks.list(None, 10).unwrap().is_empty());
}
