use super::*;
use navo_core::retention::{
    ADOPTED_CONTAINER_PREFIX, COPY_RETAINED_KIND, MOVE_RETAINED_KIND, OverwriteProof,
    ProvedOverwrite, RetainedKind, RetainedNode, RetainedWritePlan,
};
use navo_core::{Conflict, OpKind, OperationPlan, Resolution, Step, Totals};
use navo_fs::{EntryKind, FileStamp, FsEntry, HeldFile};
use navo_store::{JobJournal, JobState};

struct AdoptedFixture {
    profile: tempfile::TempDir,
    fs: FakeFs,
    entry: RetentionEntry,
}

impl AdoptedFixture {
    fn copy() -> Self {
        Self::new(false)
    }

    fn moved() -> Self {
        Self::new(true)
    }

    fn new(moved: bool) -> Self {
        let profile = tempfile::tempdir().unwrap();
        let fs = FakeFs::new();
        let container_rel = format!("home/{ADOPTED_CONTAINER_PREFIX}deadbeef");
        let held_rel = format!("{container_rel}/a");
        fs.add_folder("home");
        fs.add_folder(&container_rel);
        fs.add_file(&held_rel, 8);
        fs.add_file("home/a", 16);
        if moved {
            fs.add_folder("src");
        } else {
            fs.add_file("src/a", 16);
        }

        let source = fs.location("home/a");
        let from = fs.location("src/a");
        let held_loc = fs.location(&held_rel);
        let container_loc = fs.location(&container_rel);
        let copy_meta = fs.metadata(&source).unwrap();
        let plan = build_overwrite_plan(moved, &from, &source, copy_meta.size);
        let policy = RetentionPolicy::new(7, 1000, 0).unwrap();
        let (held_file, evidence) = build_evidence(&fs, &source, &held_loc, &container_loc);
        let job = record_adopted_job(profile.path(), moved, &plan, policy, &held_file, &evidence);

        let mut vault = open(profile.path(), &fs);
        let entries = run(|run| {
            vault.adopt_job(
                &AdoptJob {
                    job,
                    state: JobState::Done,
                    files: &[(held_file, evidence)],
                    plan: &plan,
                    policy,
                    created_at: 100,
                    keep_evidence: false,
                },
                run,
            )
        })
        .unwrap();
        assert_eq!(entries.len(), 1);
        let entry = entries.into_iter().next().unwrap();
        assert!(entry.record.replaced_by.is_some());
        Self { profile, fs, entry }
    }

    fn vault(&self) -> Vault<'_> {
        open(self.profile.path(), &self.fs)
    }

    fn parked(&self) -> Location {
        self.entry.record.parked().unwrap().unwrap()
    }

    fn held(&self) -> Location {
        self.entry.record.held().unwrap()
    }
}

fn build_overwrite_plan(
    moved: bool,
    from: &Location,
    source: &Location,
    size: u64,
) -> OperationPlan {
    OperationPlan {
        kind: if moved { OpKind::Move } else { OpKind::Copy },
        steps: vec![if moved {
            Step::Rename {
                source: from.clone(),
                target: source.clone(),
            }
        } else {
            Step::CopyFile {
                source: from.clone(),
                target: source.clone(),
                size,
            }
        }],
        conflicts: vec![Conflict {
            source: from.clone(),
            target: source.clone(),
            resolution: Resolution::Overwrite,
        }],
        totals: Totals {
            files: 1,
            folders: 0,
            bytes: size,
        },
        risks: vec![],
    }
}

fn build_evidence(
    fs: &FakeFs,
    source: &Location,
    held_loc: &Location,
    container_loc: &Location,
) -> (HeldFile, OverwriteProof) {
    let parent_id = fs.file_identity(&fs.location("home")).unwrap().unwrap();
    let container_id = fs.file_identity(container_loc).unwrap().unwrap();
    let mount = fs.mount_identity(&fs.location("home")).unwrap().unwrap();
    let held_meta = fs.metadata(held_loc).unwrap();
    let held_id = fs.file_identity(held_loc).unwrap().unwrap();
    let copy_meta = fs.metadata(source).unwrap();
    let copy_id = fs.file_identity(source).unwrap().unwrap();
    let held_file = HeldFile {
        original: source.clone(),
        held: held_loc.clone(),
        volume: Some(parent_id.volume),
        placed: Some(FileStamp {
            size: copy_meta.size,
            modified_ms: copy_meta.modified_ms,
        }),
        placed_hash: None,
    };
    let evidence = OverwriteProof {
        parent_identity: parent_id,
        container_identity: container_id,
        mount_identity: mount,
        node: RetainedNode {
            path: vec![],
            kind: RetainedKind::File,
            identity: held_id,
            size: held_meta.size,
            modified_ms: held_meta.modified_ms,
        },
        replacement_identity: copy_id,
    };
    (held_file, evidence)
}

fn record_adopted_job(
    profile: &Path,
    moved: bool,
    plan: &OperationPlan,
    policy: RetentionPolicy,
    held_file: &HeldFile,
    evidence: &OverwriteProof,
) -> JobId {
    let write_plan = RetainedWritePlan::new(plan.clone(), policy).unwrap();
    let plan_bytes = serde_json::to_vec(&write_plan).unwrap();
    let (db, _) = Db::open(&profile.join("navo.db"), false).unwrap();
    let job_kind = if moved {
        MOVE_RETAINED_KIND
    } else {
        COPY_RETAINED_KIND
    };
    let mut journal = JobJournal::new(db).unwrap();
    let job = journal.create_job(job_kind, &plan_bytes, 1, 100).unwrap();
    journal
        .mark_step(job, 0, navo_store::StepState::Done, None)
        .unwrap();
    let proved = ProvedOverwrite {
        original: held_file.original.clone(),
        held: held_file.held.clone(),
        placed_size: held_file.placed.map(|s| s.size),
        placed_modified_ms: held_file.placed.and_then(|s| s.modified_ms),
        volume: held_file.volume,
        placed_hash: None,
        proof: evidence.clone(),
    };
    journal
        .record_held(job, &serde_json::to_vec(&proved).unwrap(), 100)
        .unwrap();
    job
}

fn restore_in_place(vault: &mut Vault<'_>, id: RetentionId) -> Result<InPlace, Error> {
    run(|run| vault.restore_in_place(id, run))
}

fn fault_next(fs: &FakeFs, at: &Location, op: FakeOp, fault: Fault) {
    let path = at.path().to_text();
    let next = fs.calls(&path, op) + 1;
    fs.fault(&path, op, next, fault);
}

#[test]
fn restore_in_place_restores_original_removes_parked_copy_and_keeps_from() {
    let fx = AdoptedFixture::copy();
    let original_id = fx.fs.file_identity(&fx.held()).unwrap();
    let mut vault = fx.vault();

    let outcome = restore_in_place(&mut vault, fx.entry.id).unwrap();
    let InPlace::Restored(restored) = outcome else {
        panic!("expected InPlace::Restored");
    };
    assert_eq!(restored.state, RetainedState::Restored);
    assert_eq!(
        restored.target.as_ref().map(|t| &t.location),
        Some(&fx.entry.record.source)
    );
    assert_eq!(fx.fs.metadata(&fx.entry.record.source).unwrap().size, 8);
    assert_eq!(
        fx.fs.file_identity(&fx.entry.record.source).unwrap(),
        original_id
    );
    assert_eq!(fx.fs.metadata(&fx.fs.location("src/a")).unwrap().size, 16);
    assert!(fx.fs.metadata(&fx.parked()).is_err());
    assert!(fx.fs.metadata(&fx.entry.record.container).is_err());
}

#[test]
fn not_in_place_when_no_replacement_metadata() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.set_available_space("home", Some(u64::MAX));
    fs.add_file("home/a", 8);
    let mut vault = open(profile.path(), &fs);
    let entry = capture(&mut vault, &fs.location("home/a"));

    assert_eq!(
        restore_in_place(&mut vault, entry.id).unwrap(),
        InPlace::NotInPlace
    );
    assert_eq!(vault.get(entry.id).unwrap(), entry);
}

#[test]
fn restore_in_place_for_move_returns_moved_file_and_restores_original() {
    let fx = AdoptedFixture::moved();
    let original_id = fx.fs.file_identity(&fx.held()).unwrap();
    let moved_id = fx.fs.file_identity(&fx.entry.record.source).unwrap();
    let mut vault = fx.vault();

    let InPlace::Restored(restored) = restore_in_place(&mut vault, fx.entry.id).unwrap() else {
        panic!("expected InPlace::Restored");
    };
    assert_eq!(restored.state, RetainedState::Restored);
    assert_eq!(
        restored.target.as_ref().map(|t| &t.location),
        Some(&fx.entry.record.source)
    );
    assert_eq!(fx.fs.metadata(&fx.entry.record.source).unwrap().size, 8);
    assert_eq!(
        fx.fs.file_identity(&fx.entry.record.source).unwrap(),
        original_id
    );
    assert_eq!(fx.fs.metadata(&fx.fs.location("src/a")).unwrap().size, 16);
    assert_eq!(
        fx.fs.file_identity(&fx.fs.location("src/a")).unwrap(),
        moved_id
    );
    assert!(fx.fs.metadata(&fx.entry.record.container).is_err());
}

#[test]
fn move_not_in_place_when_from_is_occupied() {
    let fx = AdoptedFixture::moved();
    fx.fs.add_file("src/a", 99);
    let mut vault = fx.vault();

    assert_eq!(
        restore_in_place(&mut vault, fx.entry.id).unwrap(),
        InPlace::NotInPlace
    );
    assert_eq!(vault.get(fx.entry.id).unwrap(), fx.entry);
    assert_eq!(fx.fs.metadata(&fx.fs.location("src/a")).unwrap().size, 99);
    assert_eq!(fx.fs.metadata(&fx.entry.record.source).unwrap().size, 16);
    assert!(fx.fs.metadata(&fx.held()).is_ok());
}

#[test]
fn move_not_in_place_when_from_parent_folder_was_deleted() {
    let fx = AdoptedFixture::moved();
    fx.fs.remove("src");
    let mut vault = fx.vault();

    assert_eq!(
        restore_in_place(&mut vault, fx.entry.id).unwrap(),
        InPlace::NotInPlace
    );
    assert_eq!(vault.get(fx.entry.id).unwrap(), fx.entry);
    assert_eq!(fx.fs.metadata(&fx.entry.record.source).unwrap().size, 16);
    assert!(fx.fs.metadata(&fx.held()).is_ok());
}

#[test]
fn move_not_in_place_when_from_parent_is_on_different_volume() {
    let fx = AdoptedFixture::moved();
    let other_volume = fx.entry.record.parent_identity.volume.wrapping_add(1);
    fx.fs.set_volume("src", other_volume);
    let mut vault = fx.vault();

    assert_eq!(
        restore_in_place(&mut vault, fx.entry.id).unwrap(),
        InPlace::NotInPlace
    );
    assert_eq!(vault.get(fx.entry.id).unwrap(), fx.entry);
    assert_eq!(fx.fs.metadata(&fx.entry.record.source).unwrap().size, 16);
    assert!(fx.fs.metadata(&fx.held()).is_ok());
}

#[test]
fn move_step_3_rename_failure_rolls_from_back_to_source_and_sets_ready() {
    let fx = AdoptedFixture::moved();
    let moved_id = fx.fs.file_identity(&fx.entry.record.source).unwrap();
    fault_next(
        &fx.fs,
        &fx.held(),
        FakeOp::Rename,
        Fault::Fail(FsErrorKind::Locked),
    );
    let mut vault = fx.vault();

    assert!(restore_in_place(&mut vault, fx.entry.id).is_err());
    let after = vault.get(fx.entry.id).unwrap();
    assert_eq!(after.state, RetainedState::Ready);
    assert_eq!(after.target, None);
    assert_eq!(
        fx.fs.file_identity(&fx.entry.record.source).unwrap(),
        moved_id
    );
    assert!(fx.fs.metadata(&fx.fs.location("src/a")).is_err());
    assert_eq!(fx.fs.metadata(&fx.held()).unwrap().size, 8);
}

#[test]
fn step_3_double_fault_rollback_stays_restoring() {
    let fx = AdoptedFixture::moved();
    let moved_id = fx.fs.file_identity(&fx.entry.record.source).unwrap();
    fault_next(
        &fx.fs,
        &fx.held(),
        FakeOp::Rename,
        Fault::Fail(FsErrorKind::Locked),
    );
    fault_next(
        &fx.fs,
        &fx.parked(),
        FakeOp::Rename,
        Fault::Fail(FsErrorKind::Locked),
    );
    let mut vault = fx.vault();

    assert!(restore_in_place(&mut vault, fx.entry.id).is_err());
    let after = vault.get(fx.entry.id).unwrap();
    assert_eq!(after.state, RetainedState::Restoring);
    assert_eq!(
        after.target.as_ref().map(|t| &t.location),
        Some(&fx.entry.record.source)
    );
    assert!(fx.fs.metadata(&fx.entry.record.source).is_err());
    assert_eq!(fx.fs.file_identity(&fx.parked()).unwrap(), moved_id);
    assert_eq!(fx.fs.metadata(&fx.held()).unwrap().size, 8);
}

#[test]
fn move_cleanup_record_never_deletes_from() {
    let fx = AdoptedFixture::moved();
    let moved_id = fx.fs.file_identity(&fx.entry.record.source).unwrap();
    fx.fs
        .rename_atomic(&fx.entry.record.source, &fx.parked())
        .unwrap();
    fx.fs
        .rename_atomic(&fx.held(), &fx.entry.record.source)
        .unwrap();

    assert!(proof::cleanup_record(&fx.fs, &fx.entry.record).unwrap());
    assert!(fx.fs.metadata(&fx.entry.record.container).is_err());
    assert_eq!(fx.fs.file_identity(&fx.parked()).unwrap(), moved_id);
    assert_eq!(fx.fs.metadata(&fx.parked()).unwrap().size, 16);
}

#[test]
fn not_in_place_when_state_is_not_ready() {
    let fx = AdoptedFixture::copy();
    let mut vault = fx.vault();
    begin_restore(&mut vault, fx.entry.id, &fx.entry.record.source);
    let before = vault.get(fx.entry.id).unwrap();

    assert_eq!(
        restore_in_place(&mut vault, fx.entry.id).unwrap(),
        InPlace::NotInPlace
    );
    assert_eq!(vault.get(fx.entry.id).unwrap(), before);
}

#[test]
fn not_in_place_when_pinned_and_not_kept() {
    let fx = AdoptedFixture::copy();
    let job_id = fx.entry.job_id.unwrap();
    let (db, _) = Db::open(&fx.profile.path().join("navo.db"), false).unwrap();
    db.connection()
        .execute("UPDATE job SET state = 'running' WHERE id = ?1", (job_id,))
        .unwrap();
    let mut vault = fx.vault();
    let before = vault.get(fx.entry.id).unwrap();
    assert!(before.pinned && !before.kept);

    assert_eq!(
        restore_in_place(&mut vault, fx.entry.id).unwrap(),
        InPlace::NotInPlace
    );
    assert_eq!(vault.get(fx.entry.id).unwrap(), before);
}

#[test]
fn not_in_place_when_from_is_missing() {
    let fx = AdoptedFixture::copy();
    fx.fs.remove("src/a");
    let mut vault = fx.vault();

    assert_eq!(
        restore_in_place(&mut vault, fx.entry.id).unwrap(),
        InPlace::NotInPlace
    );
    assert_eq!(vault.get(fx.entry.id).unwrap(), fx.entry);
    assert_eq!(fx.fs.metadata(&fx.entry.record.source).unwrap().size, 16);
    assert!(fx.fs.metadata(&fx.held()).is_ok());
}

#[test]
fn not_in_place_when_source_was_edited_deleted_or_replaced_by_link() {
    // Edited (different size).
    let fx = AdoptedFixture::copy();
    fx.fs.remove("home/a");
    fx.fs.add_file("home/a", 24);
    let mut vault = fx.vault();
    assert_eq!(
        restore_in_place(&mut vault, fx.entry.id).unwrap(),
        InPlace::NotInPlace
    );
    assert_eq!(vault.get(fx.entry.id).unwrap(), fx.entry);

    // Deleted.
    let fx = AdoptedFixture::copy();
    fx.fs.remove("home/a");
    let mut vault = fx.vault();
    assert_eq!(
        restore_in_place(&mut vault, fx.entry.id).unwrap(),
        InPlace::NotInPlace
    );
    assert_eq!(vault.get(fx.entry.id).unwrap(), fx.entry);

    // Replaced by a symlink/link.
    let fx = AdoptedFixture::copy();
    fx.fs.remove("home/a");
    fx.fs.add_entry(
        "home",
        FsEntry {
            name: ProviderPath::from_text("a"),
            kind: EntryKind::Link,
            size: 16,
            modified_ms: None,
            attributes: Default::default(),
        },
    );
    let mut vault = fx.vault();
    assert_eq!(
        restore_in_place(&mut vault, fx.entry.id).unwrap(),
        InPlace::NotInPlace
    );
    assert_eq!(vault.get(fx.entry.id).unwrap(), fx.entry);
}

#[test]
fn not_in_place_when_parent_folder_was_renamed() {
    let fx = AdoptedFixture::copy();
    fx.fs.rename_entry("home", "home_renamed");
    let mut vault = fx.vault();

    assert_eq!(
        restore_in_place(&mut vault, fx.entry.id).unwrap(),
        InPlace::NotInPlace
    );
    assert_eq!(vault.get(fx.entry.id).unwrap(), fx.entry);
}

#[test]
fn not_in_place_when_parked_path_is_occupied() {
    let fx = AdoptedFixture::copy();
    fx.fs.add_file(&fx.parked().path().to_text(), 99);
    let mut vault = fx.vault();

    assert_eq!(
        restore_in_place(&mut vault, fx.entry.id).unwrap(),
        InPlace::NotInPlace
    );
    assert_eq!(vault.get(fx.entry.id).unwrap(), fx.entry);
    assert_eq!(fx.fs.metadata(&fx.parked()).unwrap().size, 99);
    assert_eq!(fx.fs.metadata(&fx.entry.record.source).unwrap().size, 16);
}

#[test]
fn not_in_place_when_held_file_is_missing() {
    let fx = AdoptedFixture::copy();
    fx.fs.remove(&fx.held().path().to_text());
    let mut vault = fx.vault();

    assert_eq!(
        restore_in_place(&mut vault, fx.entry.id).unwrap(),
        InPlace::NotInPlace
    );
    assert_eq!(vault.get(fx.entry.id).unwrap(), fx.entry);
    assert_eq!(fx.fs.metadata(&fx.entry.record.source).unwrap().size, 16);
}

#[test]
fn step_2_rename_failure_reverts_to_ready_and_returns_not_in_place() {
    let fx = AdoptedFixture::copy();
    fault_next(
        &fx.fs,
        &fx.entry.record.source,
        FakeOp::Rename,
        Fault::Fail(FsErrorKind::Locked),
    );
    let mut vault = fx.vault();

    assert_eq!(
        restore_in_place(&mut vault, fx.entry.id).unwrap(),
        InPlace::NotInPlace
    );
    let after = vault.get(fx.entry.id).unwrap();
    assert_eq!(after.state, RetainedState::Ready);
    assert_eq!(after.target, None);
    assert_eq!(fx.fs.metadata(&fx.entry.record.source).unwrap().size, 16);
    assert!(fx.fs.metadata(&fx.parked()).is_err());
    assert!(fx.fs.metadata(&fx.held()).is_ok());
}

#[test]
fn step_2_rename_then_fail_renames_parked_back_and_returns_not_in_place() {
    let fx = AdoptedFixture::copy();
    fault_next(
        &fx.fs,
        &fx.entry.record.source,
        FakeOp::Rename,
        Fault::RenameThenFail(FsErrorKind::Locked),
    );
    let mut vault = fx.vault();

    assert_eq!(
        restore_in_place(&mut vault, fx.entry.id).unwrap(),
        InPlace::NotInPlace
    );
    let after = vault.get(fx.entry.id).unwrap();
    assert_eq!(after.state, RetainedState::Ready);
    assert_eq!(after.target, None);
    assert_eq!(fx.fs.metadata(&fx.entry.record.source).unwrap().size, 16);
    assert!(fx.fs.metadata(&fx.parked()).is_err());
    assert!(fx.fs.metadata(&fx.held()).is_ok());
}

#[test]
fn step_3_rename_failure_rolls_back_parked_copy_sets_ready_and_returns_error() {
    let fx = AdoptedFixture::copy();
    fault_next(
        &fx.fs,
        &fx.held(),
        FakeOp::Rename,
        Fault::Fail(FsErrorKind::Locked),
    );
    let mut vault = fx.vault();

    assert!(restore_in_place(&mut vault, fx.entry.id).is_err());
    let after = vault.get(fx.entry.id).unwrap();
    assert_eq!(after.state, RetainedState::Ready);
    assert_eq!(after.target, None);
    assert_eq!(fx.fs.metadata(&fx.entry.record.source).unwrap().size, 16);
    assert!(fx.fs.metadata(&fx.parked()).is_err());
    assert_eq!(fx.fs.metadata(&fx.held()).unwrap().size, 8);
}

#[test]
fn step_3_rename_then_fail_leaves_restoring_and_reconcile_settles_to_restored() {
    let fx = AdoptedFixture::copy();
    let original_id = fx.fs.file_identity(&fx.held()).unwrap();
    fault_next(
        &fx.fs,
        &fx.held(),
        FakeOp::Rename,
        Fault::RenameThenFail(FsErrorKind::Locked),
    );
    let mut vault = fx.vault();

    assert!(restore_in_place(&mut vault, fx.entry.id).is_err());
    assert_eq!(
        vault.get(fx.entry.id).unwrap().state,
        RetainedState::Restoring
    );
    assert_eq!(
        fx.fs.file_identity(&fx.entry.record.source).unwrap(),
        original_id
    );

    let settled = reconcile(&mut vault, fx.entry.id).unwrap();
    assert_eq!(settled.state, RetainedState::Restored);
    assert_eq!(
        fx.fs.file_identity(&fx.entry.record.source).unwrap(),
        original_id
    );
    assert!(fx.fs.metadata(&fx.parked()).is_err());
    assert!(fx.fs.metadata(&fx.entry.record.container).is_err());
}

#[test]
fn step_5_cleanup_failure_still_returns_restored_and_next_reconcile_cleans_parked_copy() {
    let fx = AdoptedFixture::copy();
    fault_next(
        &fx.fs,
        &fx.parked(),
        FakeOp::Remove,
        Fault::Fail(FsErrorKind::Locked),
    );
    let mut vault = fx.vault();

    let InPlace::Restored(restored) = restore_in_place(&mut vault, fx.entry.id).unwrap() else {
        panic!("expected InPlace::Restored");
    };
    assert_eq!(restored.state, RetainedState::Restored);
    assert!(fx.fs.metadata(&fx.parked()).is_ok());

    let reconciled = reconcile(&mut vault, fx.entry.id).unwrap();
    assert_eq!(reconciled.state, RetainedState::Restored);
    assert!(fx.fs.metadata(&fx.parked()).is_err());
    assert!(fx.fs.metadata(&fx.entry.record.container).is_err());
}

#[test]
fn cleanup_record_keeps_unmatched_file_at_parked_path() {
    let fx = AdoptedFixture::copy();
    fx.fs.remove(&fx.held().path().to_text());
    fx.fs.add_file(&fx.parked().path().to_text(), 99);
    assert!(!proof::cleanup_record(&fx.fs, &fx.entry.record).unwrap());
    assert_eq!(fx.fs.metadata(&fx.parked()).unwrap().size, 99);
}

#[test]
fn reconcile_restoring_held_present_parked_absent_source_is_replacement_returns_ready() {
    let fx = AdoptedFixture::copy();
    let mut vault = fx.vault();
    begin_restore(&mut vault, fx.entry.id, &fx.entry.record.source);

    let settled = reconcile(&mut vault, fx.entry.id).unwrap();
    assert_eq!(settled.state, RetainedState::Ready);
    assert_eq!(settled.target, None);
    assert_eq!(fx.fs.metadata(&fx.entry.record.source).unwrap().size, 16);
    assert_eq!(fx.fs.metadata(&fx.held()).unwrap().size, 8);
}

#[test]
fn reconcile_restoring_held_present_parked_is_replacement_source_absent_renames_back() {
    let fx = AdoptedFixture::copy();
    let mut vault = fx.vault();
    begin_restore(&mut vault, fx.entry.id, &fx.entry.record.source);
    fx.fs
        .rename_atomic(&fx.entry.record.source, &fx.parked())
        .unwrap();

    let settled = reconcile(&mut vault, fx.entry.id).unwrap();
    assert_eq!(settled.state, RetainedState::Ready);
    assert_eq!(settled.target, None);
    assert_eq!(fx.fs.metadata(&fx.entry.record.source).unwrap().size, 16);
    assert!(fx.fs.metadata(&fx.parked()).is_err());
    assert_eq!(fx.fs.metadata(&fx.held()).unwrap().size, 8);
}

#[test]
fn reconcile_restoring_held_absent_source_is_original_returns_restored_and_cleans_parked() {
    let fx = AdoptedFixture::copy();
    let original_id = fx.fs.file_identity(&fx.held()).unwrap();
    let mut vault = fx.vault();
    begin_restore(&mut vault, fx.entry.id, &fx.entry.record.source);
    fx.fs
        .rename_atomic(&fx.entry.record.source, &fx.parked())
        .unwrap();
    fx.fs
        .rename_atomic(&fx.held(), &fx.entry.record.source)
        .unwrap();

    let settled = reconcile(&mut vault, fx.entry.id).unwrap();
    assert_eq!(settled.state, RetainedState::Restored);
    assert_eq!(
        fx.fs.file_identity(&fx.entry.record.source).unwrap(),
        original_id
    );
    assert!(fx.fs.metadata(&fx.parked()).is_err());
    assert!(fx.fs.metadata(&fx.entry.record.container).is_err());
}

#[test]
fn reconcile_restoring_unexpected_combinations_yield_conflict_without_touching_files() {
    // Held present + parked present + source ALSO present (new file appeared at source).
    let fx = AdoptedFixture::copy();
    let mut vault = fx.vault();
    begin_restore(&mut vault, fx.entry.id, &fx.entry.record.source);
    fx.fs
        .rename_atomic(&fx.entry.record.source, &fx.parked())
        .unwrap();
    fx.fs.add_file("home/a", 77);

    let settled = reconcile(&mut vault, fx.entry.id).unwrap();
    assert_eq!(settled.state, RetainedState::Conflict);
    assert_eq!(fx.fs.metadata(&fx.entry.record.source).unwrap().size, 77);
    assert_eq!(fx.fs.metadata(&fx.parked()).unwrap().size, 16);
    assert_eq!(fx.fs.metadata(&fx.held()).unwrap().size, 8);

    // Held present + parked absent + source replaced by a different file.
    let fx = AdoptedFixture::copy();
    let mut vault = fx.vault();
    begin_restore(&mut vault, fx.entry.id, &fx.entry.record.source);
    fx.fs.remove("home/a");
    fx.fs.add_file("home/a", 55);

    let settled = reconcile(&mut vault, fx.entry.id).unwrap();
    assert_eq!(settled.state, RetainedState::Conflict);
    assert_eq!(fx.fs.metadata(&fx.entry.record.source).unwrap().size, 55);
    assert_eq!(fx.fs.metadata(&fx.held()).unwrap().size, 8);

    // Held absent + source is not the original.
    let fx = AdoptedFixture::copy();
    let mut vault = fx.vault();
    begin_restore(&mut vault, fx.entry.id, &fx.entry.record.source);
    fx.fs.remove(&fx.held().path().to_text());

    let settled = reconcile(&mut vault, fx.entry.id).unwrap();
    assert_eq!(settled.state, RetainedState::Conflict);
    assert_eq!(fx.fs.metadata(&fx.entry.record.source).unwrap().size, 16);
}

#[test]
fn reconcile_move_restoring_all_four_cases() {
    // 1. Held present + from absent + source is replacement -> Ready.
    let fx = AdoptedFixture::moved();
    let mut vault = fx.vault();
    begin_restore(&mut vault, fx.entry.id, &fx.entry.record.source);

    let settled = reconcile(&mut vault, fx.entry.id).unwrap();
    assert_eq!(settled.state, RetainedState::Ready);
    assert_eq!(settled.target, None);
    assert_eq!(fx.fs.metadata(&fx.entry.record.source).unwrap().size, 16);
    assert!(fx.fs.metadata(&fx.parked()).is_err());
    assert_eq!(fx.fs.metadata(&fx.held()).unwrap().size, 8);

    // 2. Held present + from is replacement + source absent -> rename from back to source -> Ready.
    let fx = AdoptedFixture::moved();
    let moved_id = fx.fs.file_identity(&fx.entry.record.source).unwrap();
    let mut vault = fx.vault();
    begin_restore(&mut vault, fx.entry.id, &fx.entry.record.source);
    fx.fs
        .rename_atomic(&fx.entry.record.source, &fx.parked())
        .unwrap();

    let settled = reconcile(&mut vault, fx.entry.id).unwrap();
    assert_eq!(settled.state, RetainedState::Ready);
    assert_eq!(settled.target, None);
    assert_eq!(
        fx.fs.file_identity(&fx.entry.record.source).unwrap(),
        moved_id
    );
    assert!(fx.fs.metadata(&fx.parked()).is_err());
    assert_eq!(fx.fs.metadata(&fx.held()).unwrap().size, 8);

    // 3. Held absent + source is original -> Restored, container removed, from left intact!
    let fx = AdoptedFixture::moved();
    let original_id = fx.fs.file_identity(&fx.held()).unwrap();
    let moved_id = fx.fs.file_identity(&fx.entry.record.source).unwrap();
    let mut vault = fx.vault();
    begin_restore(&mut vault, fx.entry.id, &fx.entry.record.source);
    fx.fs
        .rename_atomic(&fx.entry.record.source, &fx.parked())
        .unwrap();
    fx.fs
        .rename_atomic(&fx.held(), &fx.entry.record.source)
        .unwrap();

    let settled = reconcile(&mut vault, fx.entry.id).unwrap();
    assert_eq!(settled.state, RetainedState::Restored);
    assert_eq!(
        fx.fs.file_identity(&fx.entry.record.source).unwrap(),
        original_id
    );
    assert_eq!(fx.fs.file_identity(&fx.parked()).unwrap(), moved_id);
    assert_eq!(fx.fs.metadata(&fx.parked()).unwrap().size, 16);
    assert!(fx.fs.metadata(&fx.entry.record.container).is_err());

    // 4. Unexpected combination (held present + from present + new file at source) -> Conflict, untouched.
    let fx = AdoptedFixture::moved();
    let mut vault = fx.vault();
    begin_restore(&mut vault, fx.entry.id, &fx.entry.record.source);
    fx.fs
        .rename_atomic(&fx.entry.record.source, &fx.parked())
        .unwrap();
    fx.fs.add_file("home/a", 77);

    let settled = reconcile(&mut vault, fx.entry.id).unwrap();
    assert_eq!(settled.state, RetainedState::Conflict);
    assert_eq!(fx.fs.metadata(&fx.entry.record.source).unwrap().size, 77);
    assert_eq!(fx.fs.metadata(&fx.parked()).unwrap().size, 16);
    assert_eq!(fx.fs.metadata(&fx.held()).unwrap().size, 8);
}
