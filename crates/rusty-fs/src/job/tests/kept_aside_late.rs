use rusty_core::{Location, ProviderPath};

use super::kept_aside::{entry_at, keeping, old_entry, over_old};
use super::*;

fn overwritten(workers: usize) -> (FakeFs, OperationPlan, Harness) {
    let (fs, plan) = over_old(&["a.txt"]);
    let mut harness = keeping(workers);
    assert_eq!(harness.run(&fs, &plan, &[]), JobOutcome::Done);
    (fs, plan, harness)
}

fn undone(fs: &FakeFs, plan: &OperationPlan, harness: &Harness) -> (JobOutcome, Log) {
    let mut undo = Harness {
        held: harness.log.held.clone(),
        ..Harness::default()
    };
    let outcome = undo.roll_back(fs, plan, &harness.log.done());
    (outcome, undo.log)
}

fn as_old(fs: &FakeFs, path: &str) -> Option<FsEntry> {
    entry_at(fs, path).map(|entry| FsEntry {
        name: old_entry("a.txt").name,
        ..entry
    })
}

#[test]
fn a_late_restore_keeps_a_changed_target_and_puts_the_old_file_beside_it() {
    for workers in MODES {
        let (fs, plan, harness) = overwritten(workers);
        fs.remove("dst/a.txt");
        fs.add_file("dst/a.txt", 5);
        let (outcome, log) = undone(&fs, &plan, &harness);
        assert_eq!(
            (
                outcome,
                size(&fs, "dst/a.txt"),
                as_old(&fs, "dst/a (before copy).txt"),
                log.beside,
            ),
            (
                JobOutcome::Done,
                Some(5),
                Some(old_entry("a.txt")),
                vec![fs.location("dst/a (before copy).txt")]
            ),
            "workers {workers}"
        );
    }
}

#[test]
fn a_taken_beside_name_moves_on_to_the_next_number() {
    let (fs, plan, harness) = overwritten(0);
    fs.remove("dst/a.txt");
    fs.add_file("dst/a.txt", 5);
    fs.add_file("dst/a (before copy).txt", 1);
    let _ = undone(&fs, &plan, &harness);
    assert_eq!(
        (
            size(&fs, "dst/a (before copy).txt"),
            as_old(&fs, "dst/a (before copy 2).txt"),
        ),
        (Some(1), Some(old_entry("a.txt")))
    );
}

#[test]
fn a_late_restore_never_undoes_a_later_job_over_the_same_file() {
    for workers in MODES {
        let (fs, plan, first) = overwritten(workers);
        fs.add_file("src/a.txt", 77);
        let later = plan_with(&fs, &["src/a.txt"], ConflictPolicy::OverwriteAll);
        let (outcome, _) = run_in(workers, &fs, &later);
        assert_eq!(outcome, JobOutcome::Done);
        let _ = undone(&fs, &plan, &first);
        assert_eq!(
            (
                size(&fs, "dst/a.txt"),
                as_old(&fs, "dst/a (before copy).txt")
            ),
            (Some(77), Some(old_entry("a.txt"))),
            "workers {workers}"
        );
    }
}

#[test]
fn undo_with_the_source_volume_missing_keeps_the_held_file() {
    for workers in MODES {
        let (fs, plan, harness) = overwritten(workers);
        fs.remove("src");
        let (outcome, _) = undone(&fs, &plan, &harness);
        let held = harness.log.held[0].held.path().to_text();
        assert_eq!(
            (
                failure(&outcome).map(|(_, kind)| kind),
                size(&fs, "dst/a.txt"),
                size(&fs, &held),
            ),
            (Some(FsErrorKind::NotFound), Some(BIG), Some(3)),
            "workers {workers}"
        );
    }
}

#[test]
fn a_held_file_missing_with_its_folder_and_its_target_is_not_taken_as_gone() {
    for workers in MODES {
        let (fs, plan, harness) = overwritten(workers);
        let folder = harness.log.held[0].held.parent().unwrap();
        fs.remove(&folder.path().to_text());
        fs.remove("dst/a.txt");
        let released = release_held(&harness.log.held, &fs).map_err(|e| e.kind);
        let (outcome, _) = undone(&fs, &plan, &harness);
        assert_eq!(
            (released, failure(&outcome).map(|(_, kind)| kind)),
            (Err(FsErrorKind::NotFound), Some(FsErrorKind::NotFound)),
            "workers {workers}"
        );
    }
}

fn windows_style(path: &str) -> Location {
    Location::local(ProviderPath::from_text(path)).unwrap()
}

#[test]
fn a_held_record_with_the_exact_name_wins_over_a_case_twin() {
    let held = |original: &str| HeldFile {
        original: windows_style(original),
        held: windows_style(r"C:\dst\.ferrite-held-x\a.txt"),
        placed: FileStamp {
            size: 1,
            modified_ms: None,
        },
    };
    let records = [held(r"C:\dst\A.txt"), held(r"C:\dst\a.txt")];
    let step = Step::CopyFile {
        source: windows_style(r"C:\src\a.txt"),
        target: windows_style(r"C:\dst\a.txt"),
        size: 1,
    };
    let found = super::super::undo::held_for(&step, &records).map(|file| &file.original);
    assert_eq!(found, Some(&windows_style(r"C:\dst\a.txt")));
}
