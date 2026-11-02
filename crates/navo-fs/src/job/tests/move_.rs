use super::*;

fn move_plan(fs: &FakeFs) -> OperationPlan {
    plan(&request(fs, OpKind::Move, &["src/b"], "dst"))
}

fn with_link() -> FakeFs {
    let fs = fixture(&[("src/b/c.txt", BIG), ("src/b/d.txt", 6_000)]);
    add_link(&fs, "src/b", "l");
    fs
}

fn failed_move(workers: usize) -> (FakeFs, Snapshot, JobOutcome) {
    let fs = with_link();
    let plan = move_plan(&fs);
    let before = tree_of(&fs, "src");
    let fault = Fault::FailAfterBytes {
        kind: FsErrorKind::NetworkLost,
        bytes: 5_000,
    };
    fs.fault("src/b/d.txt", FakeOp::Copy, 1, fault);
    let (outcome, _) = run_in(workers, &fs, &plan);
    (fs, before, outcome)
}

#[test]
fn a_move_that_fails_while_copying_removes_no_source() {
    for workers in MODES {
        let (fs, before, _) = failed_move(workers);
        assert_eq!(tree_of(&fs, "src"), before);
    }
}

#[test]
fn a_move_that_fails_while_copying_fails_at_the_copy() {
    for workers in MODES {
        let (_, _, outcome) = failed_move(workers);
        assert_eq!(failure(&outcome), Some((2, FsErrorKind::NetworkLost)));
    }
}

#[test]
fn a_move_that_fails_while_copying_leaves_no_partial_file() {
    for workers in MODES {
        let (fs, _, _) = failed_move(workers);
        assert_eq!(
            tree_of(&fs, "dst"),
            [
                entry("b", EntryKind::Folder, 0),
                entry("b/c.txt", EntryKind::File, BIG),
            ]
        );
    }
}

#[test]
fn a_move_removes_every_source_and_keeps_links() {
    for workers in MODES {
        let fs = with_link();
        let plan = move_plan(&fs);
        let _ = run_in(workers, &fs, &plan);
        assert_eq!(
            tree_of(&fs, "src"),
            [
                entry("b", EntryKind::Folder, 0),
                entry("b/l", EntryKind::Link, 0),
            ]
        );
    }
}

#[test]
fn a_move_copies_every_file() {
    for workers in MODES {
        let fs = with_link();
        let plan = move_plan(&fs);
        let _ = run_in(workers, &fs, &plan);
        assert_eq!(
            tree_of(&fs, "dst"),
            [
                entry("b", EntryKind::Folder, 0),
                entry("b/c.txt", EntryKind::File, BIG),
                entry("b/d.txt", EntryKind::File, 6_000),
            ]
        );
    }
}

#[test]
fn a_move_without_links_removes_the_source_folder() {
    for workers in MODES {
        let fs = fixture(&[("src/b/c.txt", BIG)]);
        let plan = move_plan(&fs);
        let (outcome, _) = run_in(workers, &fs, &plan);
        assert_eq!(
            (outcome, tree_of(&fs, "src")),
            (JobOutcome::Done, Vec::new())
        );
    }
}

#[test]
fn a_move_keeps_a_source_whose_target_appeared_after_planning() {
    for workers in MODES {
        let fs = fixture(&[("src/b/c.txt", BIG), ("src/b/d.txt", 6_000)]);
        let plan = move_plan(&fs);
        fs.add_file("dst/b/d.txt", 3);
        let (outcome, _) = run_in(workers, &fs, &plan);
        assert_eq!(outcome, JobOutcome::Done);
        assert_eq!(size(&fs, "src/b/d.txt"), Some(6_000));
        assert_eq!(size(&fs, "dst/b/d.txt"), Some(3));
        assert_eq!(size(&fs, "src/b/c.txt"), None);
        assert_eq!(size(&fs, "dst/b/c.txt"), Some(BIG));
    }
}

#[test]
fn a_move_onto_an_existing_name_keeps_both_files() {
    for workers in MODES {
        let fs = fixture(&[("src/a.txt", 10_000)]);
        fs.add_file("dst/a.txt", 3);
        let plan = plan(&request(&fs, OpKind::Move, &["src/a.txt"], "dst"));
        let _ = run_in(workers, &fs, &plan);
        assert_eq!(size(&fs, "src/a.txt"), Some(10_000));
        assert_eq!(size(&fs, "dst/a.txt"), Some(3));
    }
}
