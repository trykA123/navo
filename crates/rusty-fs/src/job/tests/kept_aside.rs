use super::crashing::Crashing;
use super::*;

const OLD: u64 = 3;

pub(super) fn old_entry(name: &str) -> FsEntry {
    FsEntry {
        name: rusty_core::ProviderPath::from_text(name),
        kind: EntryKind::File,
        size: OLD,
        modified_ms: Some(1_000),
        attributes: Attributes {
            readonly: true,
            hidden: true,
            ..Attributes::default()
        },
    }
}

pub(super) fn over_old(names: &[&str]) -> (FakeFs, OperationPlan) {
    let fs = fixture(&[]);
    for name in names {
        fs.add_file(&format!("src/{name}"), BIG);
        fs.add_entry("dst", old_entry(name));
    }
    let sources: Vec<String> = names.iter().map(|name| format!("src/{name}")).collect();
    let sources: Vec<&str> = sources.iter().map(String::as_str).collect();
    let plan = plan_with(&fs, &sources, ConflictPolicy::OverwriteAll);
    (fs, plan)
}

pub(super) fn entry_at(fs: &FakeFs, path: &str) -> Option<FsEntry> {
    fs.metadata(&fs.location(path)).ok()
}

pub(super) fn holding(fs: &FakeFs) -> Vec<String> {
    tree_of(fs, "")
        .into_iter()
        .map(|(path, _, _)| path)
        .filter(|path| path.contains(HELD_PREFIX))
        .collect()
}

pub(super) fn keeping(workers: usize) -> Harness {
    Harness {
        workers,
        keeps_held: true,
        ..Harness::default()
    }
}

fn overwritten(workers: usize) -> (FakeFs, OperationPlan, Harness) {
    let (fs, plan) = over_old(&["a.txt"]);
    let mut harness = keeping(workers);
    assert_eq!(harness.run(&fs, &plan, &[]), JobOutcome::Done);
    (fs, plan, harness)
}

fn undone(fs: &FakeFs, plan: &OperationPlan, harness: &Harness) -> JobOutcome {
    let mut undo = Harness {
        held: harness.log.held.clone(),
        ..Harness::default()
    };
    undo.roll_back(fs, plan, &harness.log.done())
}

#[test]
fn undo_after_an_overwrite_restores_the_old_file_with_its_attributes() {
    for workers in MODES {
        let (fs, plan, harness) = overwritten(workers);
        assert_eq!(size(&fs, "dst/a.txt"), Some(BIG));
        assert_eq!(undone(&fs, &plan, &harness), JobOutcome::Done);
        assert_eq!(
            entry_at(&fs, "dst/a.txt"),
            Some(old_entry("a.txt")),
            "workers {workers}"
        );
    }
}

#[test]
fn undo_then_release_leaves_the_starting_tree() {
    for workers in MODES {
        let (fs, plan) = over_old(&["a.txt"]);
        let before = tree_of(&fs, "");
        let mut harness = keeping(workers);
        let _ = harness.run(&fs, &plan, &[]);
        let _ = undone(&fs, &plan, &harness);
        assert_eq!(release_held(&harness.log.held, &fs), Ok(()));
        assert_eq!(tree_of(&fs, ""), before, "workers {workers}");
    }
}

#[test]
fn the_old_file_is_held_in_a_job_folder_next_to_the_target() {
    for workers in MODES {
        let (fs, _, harness) = overwritten(workers);
        let [held] = harness.log.held.as_slice() else {
            panic!("one held file: {:?}", harness.log.held);
        };
        let folder = held.held.parent().unwrap();
        let folder_name = folder.name().unwrap().to_text();
        assert_eq!(
            (
                held.original.clone(),
                folder.parent(),
                folder_name.starts_with(HELD_PREFIX),
                held.held.name().unwrap().to_text(),
                fs.metadata(&held.held).ok(),
            ),
            (
                fs.location("dst/a.txt"),
                Some(fs.location("dst")),
                true,
                "a.txt".to_owned(),
                Some(old_entry("a.txt")),
            ),
            "workers {workers}"
        );
    }
}

struct Watching<'a> {
    fs: &'a FakeFs,
    seen: Vec<Option<u64>>,
}

impl JobLog for Watching<'_> {
    fn step(&mut self, _: u32, _: StepOutcome) -> Result<(), FsError> {
        Ok(())
    }

    fn journal_held(&mut self, held: &HeldFile) -> Result<bool, FsError> {
        self.seen
            .push(self.fs.metadata(&held.original).ok().map(|e| e.size));
        Ok(true)
    }
}

#[test]
fn the_old_file_is_journaled_while_it_is_still_at_the_target() {
    for workers in MODES {
        let (fs, plan) = over_old(&["a.txt"]);
        let mut log = Watching {
            fs: &fs,
            seen: Vec::new(),
        };
        let mut ctx = JobContext {
            ops: &fs,
            log: &mut log,
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
        };
        let _ = run_job_parallel(&plan, &[], &mut ctx, workers);
        assert_eq!(log.seen, vec![Some(OLD)], "workers {workers}");
    }
}

#[test]
fn release_removes_the_held_file_and_its_folder() {
    for workers in MODES {
        let (fs, _, harness) = overwritten(workers);
        assert_eq!(release_held(&harness.log.held, &fs), Ok(()));
        assert_eq!(
            tree_of(&fs, "dst"),
            [entry("a.txt", EntryKind::File, BIG)],
            "workers {workers}"
        );
    }
}

#[test]
fn release_puts_a_held_file_back_when_its_target_is_gone() {
    for workers in MODES {
        let (fs, _, harness) = overwritten(workers);
        fs.remove("dst/a.txt");
        assert_eq!(release_held(&harness.log.held, &fs), Ok(()));
        assert_eq!(
            (entry_at(&fs, "dst/a.txt"), holding(&fs)),
            (Some(old_entry("a.txt")), Vec::<String>::new()),
            "workers {workers}"
        );
    }
}

#[test]
fn a_failed_release_reports_the_error_and_keeps_the_held_file() {
    let (fs, _, harness) = overwritten(0);
    let held = harness.log.held[0].held.clone();
    let path = held.path().to_text();
    fs.fault(&path, FakeOp::Remove, 1, Fault::Fail(FsErrorKind::Locked));
    let released = release_held(&harness.log.held, &fs);
    assert_eq!(
        (released.map_err(|e| e.kind), fs.metadata(&held).is_ok()),
        (Err(FsErrorKind::Locked), true)
    );
}

#[test]
fn two_jobs_over_the_same_target_hold_in_separate_folders() {
    for workers in MODES {
        let (fs, plan, first) = overwritten(workers);
        let mut second = keeping(workers);
        let _ = second.run(&fs, &plan, &[]);
        let (one, two) = (&first.log.held[0].held, &second.log.held[0].held);
        assert_ne!(one.parent(), two.parent(), "workers {workers}");
        assert_eq!(
            (
                size(&fs, &one.path().to_text()),
                size(&fs, &two.path().to_text())
            ),
            (Some(OLD), Some(BIG)),
            "workers {workers}"
        );
    }
}

#[test]
fn every_overwritten_file_of_a_job_gets_its_own_held_name() {
    for workers in MODES {
        let (fs, plan) = over_old(&["a.txt", "b.txt"]);
        let mut harness = keeping(workers);
        let _ = harness.run(&fs, &plan, &[]);
        let copies = copy_targets(&plan).len();
        let mut names: Vec<String> = harness
            .log
            .held
            .iter()
            .map(|file| file.held.path().to_text())
            .collect();
        names.sort();
        names.dedup();
        let _ = undone(&fs, &plan, &harness);
        assert_eq!(
            (
                names.len(),
                entry_at(&fs, "dst/a.txt"),
                entry_at(&fs, "dst/b.txt"),
            ),
            (copies, Some(old_entry("a.txt")), Some(old_entry("b.txt"))),
            "workers {workers}"
        );
    }
}

#[test]
fn without_a_journal_an_overwrite_replaces_in_place() {
    for workers in MODES {
        let (fs, plan) = over_old(&["a.txt"]);
        let mut harness = Harness {
            workers,
            log: Log {
                unjournaled: true,
                ..Log::default()
            },
            ..Harness::default()
        };
        let outcome = harness.run(&fs, &plan, &[]);
        assert_eq!(
            (
                outcome,
                fs.calls("dst/a.txt", FakeOp::Rename),
                fs.calls(&temp("dst/a.txt"), FakeOp::Replace),
                holding(&fs),
            ),
            (JobOutcome::Done, 0, 1, Vec::<String>::new()),
            "workers {workers}"
        );
    }
}

#[test]
fn a_holding_folder_that_cannot_be_made_refuses_the_overwrite() {
    for workers in MODES {
        let (fs, plan) = over_old(&["a.txt"]);
        let refusing = Crashing::inside_holding(&fs, FakeOp::Create).refusing();
        let (outcome, harness) = run_in(workers, &refusing, &plan);
        assert_eq!(
            (
                failure(&outcome),
                harness.log.done(),
                entry_at(&fs, "dst/a.txt"),
                leftovers(&fs),
                holding(&fs),
            ),
            (
                Some((0, FsErrorKind::AccessDenied)),
                Vec::new(),
                Some(old_entry("a.txt")),
                Vec::new(),
                Vec::new()
            ),
            "workers {workers}"
        );
    }
}

#[test]
fn a_failed_hold_keeps_the_old_target() {
    for workers in MODES {
        let (fs, plan) = over_old(&["a.txt"]);
        fs.fault(
            "dst/a.txt",
            FakeOp::Rename,
            1,
            Fault::Fail(FsErrorKind::Locked),
        );
        let (outcome, _) = run_in(workers, &fs, &plan);
        assert_eq!(
            (
                failure(&outcome),
                entry_at(&fs, "dst/a.txt"),
                leftovers(&fs),
                holding(&fs),
            ),
            (
                Some((0, FsErrorKind::Locked)),
                Some(old_entry("a.txt")),
                Vec::new(),
                Vec::new()
            ),
            "workers {workers}"
        );
    }
}

#[test]
fn undo_of_a_move_whose_source_is_gone_keeps_the_moved_file_and_the_held_one() {
    for workers in MODES {
        let fs = fixture(&[("src/a.txt", BIG)]);
        fs.add_entry("dst", old_entry("a.txt"));
        let plan = plan(&PlanRequest {
            policy: ConflictPolicy::OverwriteAll,
            ..request(&fs, OpKind::Move, &["src/a.txt"], "dst")
        });
        let mut harness = keeping(workers);
        let _ = harness.run(&fs, &plan, &[]);
        let _ = undone(&fs, &plan, &harness);
        let held = &harness.log.held[0].held;
        assert_eq!(
            (size(&fs, "dst/a.txt"), fs.metadata(held).ok()),
            (Some(BIG), Some(old_entry("a.txt"))),
            "workers {workers}"
        );
    }
}

#[test]
fn an_overwrite_whose_target_is_gone_copies_without_holding() {
    for workers in MODES {
        let (fs, plan) = over_old(&["a.txt"]);
        fs.remove("dst/a.txt");
        let (outcome, harness) = run_in(workers, &fs, &plan);
        assert_eq!(
            (outcome, size(&fs, "dst/a.txt"), harness.log.held.len()),
            (JobOutcome::Done, Some(BIG), 0),
            "workers {workers}"
        );
    }
}

fn held_path(harness: &Harness, index: usize) -> String {
    harness.log.held[index].held.path().to_text()
}

#[test]
fn undo_restores_every_held_file_even_when_one_restore_fails() {
    for workers in MODES {
        let (fs, plan) = over_old(&["a.txt", "b.txt"]);
        let mut harness = keeping(workers);
        let _ = harness.run(&fs, &plan, &[]);
        let b = harness
            .log
            .held
            .iter()
            .position(|file| file.original == fs.location("dst/b.txt"))
            .unwrap();
        let sharing = Fault::Fail(FsErrorKind::SharingViolation);
        fs.fault(&held_path(&harness, b), FakeOp::Replace, 1, sharing);
        let outcome = undone(&fs, &plan, &harness);
        assert_eq!(
            (
                failure(&outcome).map(|(_, kind)| kind),
                entry_at(&fs, "dst/a.txt"),
                size(&fs, &held_path(&harness, b)),
            ),
            (
                Some(FsErrorKind::SharingViolation),
                Some(old_entry("a.txt")),
                Some(OLD)
            ),
            "workers {workers}"
        );
    }
}

#[test]
fn a_missing_target_folder_is_not_taken_for_a_released_file() {
    for workers in MODES {
        let (fs, plan, harness) = overwritten(workers);
        fs.remove("dst");
        let released = release_held(&harness.log.held, &fs).map_err(|e| e.kind);
        let restored = failure(&undone(&fs, &plan, &harness)).map(|(_, kind)| kind);
        assert_eq!(
            (released, restored),
            (Err(FsErrorKind::NotFound), Some(FsErrorKind::NotFound)),
            "workers {workers}"
        );
    }
}

#[test]
fn a_target_that_became_a_folder_is_never_held() {
    for workers in MODES {
        let (fs, plan) = over_old(&["a.txt"]);
        fs.remove("dst/a.txt");
        fs.add_folder("dst/a.txt");
        let (outcome, harness) = run_in(workers, &fs, &plan);
        assert_eq!(
            (
                failure(&outcome),
                entry_at(&fs, "dst/a.txt").map(|e| e.kind),
                harness.log.held.len(),
                holding(&fs),
                leftovers(&fs),
            ),
            (
                Some((0, FsErrorKind::NameInvalid)),
                Some(EntryKind::Folder),
                0,
                Vec::new(),
                Vec::new()
            ),
            "workers {workers}"
        );
    }
}

#[test]
fn a_stray_file_in_the_holding_folder_does_not_block_the_release() {
    for workers in MODES {
        let (fs, _, harness) = overwritten(workers);
        let folder = harness.log.held[0].held.parent().unwrap();
        fs.add_file(&format!("{}/Thumbs.db", folder.path().to_text()), 1);
        assert_eq!(
            (
                release_held(&harness.log.held, &fs),
                size(&fs, &held_path(&harness, 0)),
            ),
            (Ok(()), None),
            "workers {workers}"
        );
    }
}

#[test]
fn undo_ignores_a_held_record_whose_name_differs_only_in_case() {
    let (fs, plan, harness) = overwritten(0);
    let other_case = HeldFile {
        original: fs.location("dst/A.txt"),
        ..harness.log.held[0].clone()
    };
    let mut undo = Harness {
        held: vec![other_case],
        ..Harness::default()
    };
    let _ = undo.roll_back(&fs, &plan, &harness.log.done());
    assert_eq!(
        (size(&fs, "dst/a.txt"), size(&fs, &held_path(&harness, 0))),
        (None, Some(OLD))
    );
}
