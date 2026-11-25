//! A move copy that startup recovery keeps at its target reaches the user once, as a startup
//! notice (MR1e).

use super::*;

use navo_core::{ConflictPolicy, Location, OpKind, PlanRequest, ProviderPath, Step, plan};
use navo_fs::{Attributes, CancelToken, EntryKind, FsEntry, Provider, scan};
use navo_store::StepState;

fn file(name: &str) -> FsEntry {
    FsEntry {
        name: ProviderPath::from_text(name),
        kind: EntryKind::File,
        size: 7,
        modified_ms: Some(9_000),
        attributes: Attributes::default(),
    }
}

/// A move to another drive that stopped after its copy, journaled without a source proof (an
/// older build): the source path cannot prove it still holds the original, so the copy stays.
fn crashed_move(data: &Path, fs: &FakeFs) -> Location {
    fs.add_entry("home", file("a"));
    fs.add_folder("dest");
    let plan = plan(&PlanRequest {
        kind: OpKind::Move,
        items: scan(fs, &[fs.location("home/a")], &CancelToken::new()).unwrap(),
        target: Some(fs.location("dest")),
        existing: Vec::new(),
        policy: ConflictPolicy::Ask,
        same_volume: false,
        protected: Vec::new(),
    });
    let copy = (0_u32..)
        .zip(&plan.steps)
        .find_map(|(seq, step)| matches!(step, Step::CopyFile { .. }).then_some(seq))
        .unwrap();
    fs.add_entry("dest", file("a"));
    let journal = crate::jobs::open(&database(data)).unwrap();
    let mut journal = crate::jobs::lock(&journal);
    let steps = u32::try_from(plan.steps.len()).unwrap();
    let job = journal
        .create_job("move", &serde_json::to_vec(&plan).unwrap(), steps, 1)
        .unwrap();
    journal.mark_step(job, copy, StepState::Done, None).unwrap();
    fs.location("dest/a")
}

#[test]
fn startup_recovery_reports_the_move_copy_it_kept() {
    let data = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    let copy = crashed_move(data.path(), &fs);
    let journal = crate::jobs::open(&database(data.path())).unwrap();
    let mut kept = Vec::new();
    crate::jobs::recover_keeping(&journal, &fs, (data.path(), &[]), (false, &mut kept)).unwrap();
    assert_eq!(kept, std::slice::from_ref(&copy));
    assert!(fs.metadata(&copy).is_ok(), "the kept copy stays");
    assert!(fs.metadata(&fs.location("home/a")).is_ok());
}

#[test]
fn startup_recovery_shows_a_kept_move_copy_once_in_the_notice() {
    for experimental in [false, true] {
        let data = tempfile::tempdir().unwrap();
        let fs = FakeFs::new();
        let copy = crashed_move(data.path(), &fs);
        let start = check_with(Some(data.path()), true, &fs, experimental);
        let content = start
            .notice
            .as_ref()
            .map(RecoveryNotice::content)
            .unwrap_or_default();
        assert_eq!(content.title, navo_l10n::tr("job-recovery-kept-title"));
        assert!(
            content.details.iter().any(|line| line == copy.as_str()),
            "experimental {experimental}: {content:?}"
        );
        assert!(fs.metadata(&copy).is_ok());
        drop(start);
        let next = check_with(Some(data.path()), false, &fs, experimental);
        assert!(
            next.notice.is_none(),
            "experimental {experimental}: shown once"
        );
    }
}

#[test]
fn startup_recovery_after_a_rebuild_shows_the_move_copy_it_kept() {
    let data = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    let copy = crashed_move(data.path(), &fs);
    damaged(data.path());
    let start = check_with(Some(data.path()), true, &fs, true);
    let content = start.notice.as_ref().map(RecoveryNotice::content).unwrap();
    assert_eq!(content.title, navo_l10n::tr("store-recovery-title"));
    assert!(
        content.details.iter().any(|line| line == copy.as_str()),
        "{content:?}"
    );
}

/// A startup that fails after recovery already kept a copy still names that copy: the job is
/// finished, so no later start reports it.
#[test]
fn startup_recovery_that_fails_afterwards_still_shows_the_move_copy_it_kept() {
    let data = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    let copy = crashed_move(data.path(), &fs);
    fs::create_dir(crate::crash::sentinel_path(&database(data.path()))).unwrap();
    let start = check_with(Some(data.path()), true, &fs, true);
    let content = start.notice.as_ref().map(RecoveryNotice::content).unwrap();
    assert_eq!(content.title, navo_l10n::tr("store-recovery-blocked"));
    assert!(
        content.details.iter().any(|line| line == copy.as_str()),
        "{content:?}"
    );
}

#[test]
fn a_copy_kept_twice_in_one_start_is_listed_once() {
    let data = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    let [copy, first, last] = ["dest/a", "dest/b", "dest/c"].map(|path| fs.location(path));
    let earlier = vec![copy.clone(), first.clone(), copy.clone()];
    let notice = RecoveryNotice::with_kept(None, data.path(), earlier);
    let notice = RecoveryNotice::with_kept(notice, data.path(), vec![last.clone(), copy.clone()]);
    let content = notice.as_ref().map(RecoveryNotice::content).unwrap();
    for kept in [&copy, &first, &last] {
        let listed = content.details.iter().filter(|line| *line == kept.as_str());
        assert_eq!(listed.count(), 1, "{kept:?}: {content:?}");
    }
}
