use super::kept_aside::{keeping, over_old};
use super::*;

fn overwritten(workers: usize, target_volume: Option<u64>) -> (FakeFs, OperationPlan, Harness) {
    let (fs, plan) = over_old(&["a.txt"]);
    if let Some(serial) = target_volume {
        fs.set_volume("dst", serial);
    }
    let mut harness = keeping(workers);
    assert_eq!(harness.run(&fs, &plan, &[]), JobOutcome::Done);
    (fs, plan, harness)
}

fn undone(
    fs: &FakeFs,
    plan: &OperationPlan,
    held: &[HeldFile],
    done: &[u32],
) -> Option<FsErrorKind> {
    let mut undo = Harness {
        held: held.to_vec(),
        ..Harness::default()
    };
    failure(&undo.roll_back(fs, plan, done)).map(|(_, kind)| kind)
}

fn verdicts(
    fs: &FakeFs,
    plan: &OperationPlan,
    harness: &Harness,
) -> (Result<(), FsErrorKind>, Option<FsErrorKind>) {
    (
        release_held(&harness.log.held, fs).map_err(|e| e.kind),
        undone(fs, plan, &harness.log.held, &harness.log.done()),
    )
}

#[test]
fn a_held_row_journals_the_volume_of_the_target_folder() {
    for workers in MODES {
        let (_, _, harness) = overwritten(workers, Some(9));
        assert_eq!(harness.log.held[0].volume, Some(9), "workers {workers}");
    }
}

#[test]
fn a_target_folder_deleted_on_its_own_volume_counts_as_gone() {
    for workers in MODES {
        let (fs, plan, harness) = overwritten(workers, None);
        fs.remove("dst");
        assert_eq!(
            verdicts(&fs, &plan, &harness),
            (Ok(()), None),
            "workers {workers}"
        );
    }
}

#[test]
fn a_missing_mount_point_is_not_taken_for_a_deleted_folder() {
    for workers in MODES {
        let (fs, plan, harness) = overwritten(workers, Some(2));
        fs.remove("dst");
        assert_eq!(
            verdicts(&fs, &plan, &harness),
            (Err(FsErrorKind::NotFound), Some(FsErrorKind::NotFound)),
            "workers {workers}"
        );
    }
}

#[test]
fn a_held_row_without_a_volume_keeps_waiting() {
    for workers in MODES {
        let (fs, plan, mut harness) = overwritten(workers, None);
        harness.log.held[0].volume = None;
        fs.remove("dst");
        assert_eq!(
            verdicts(&fs, &plan, &harness),
            (Err(FsErrorKind::NotFound), Some(FsErrorKind::NotFound)),
            "workers {workers}"
        );
    }
}

#[test]
fn a_source_folder_deleted_on_the_journaled_volume_puts_the_old_file_beside() {
    for workers in MODES {
        let (fs, plan, harness) = overwritten(workers, None);
        fs.remove("src");
        assert_eq!(
            undone(&fs, &plan, &harness.log.held, &harness.log.done()),
            None,
            "workers {workers}"
        );
        assert_eq!(
            size(&fs, "dst/a (before copy).txt"),
            Some(3),
            "workers {workers}"
        );
    }
}

#[test]
fn a_missing_source_folder_on_another_volume_keeps_the_job_waiting() {
    for workers in MODES {
        let (fs, plan, harness) = overwritten(workers, Some(2));
        fs.remove("src");
        assert_eq!(
            undone(&fs, &plan, &harness.log.held, &harness.log.done()),
            Some(FsErrorKind::NotFound),
            "workers {workers}"
        );
        assert_eq!(
            size(&fs, "dst/a (before copy).txt"),
            None,
            "workers {workers}"
        );
    }
}
