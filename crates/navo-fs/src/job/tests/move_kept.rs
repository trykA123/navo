//! A move whose source must stay in place goes on with the other sources and reports each kept
//! one on its own, with the reason it was kept.

use navo_core::{Location, ProviderId, Totals};

use super::move_races::{REPLACEMENT, move_plan, run_changing};
use super::*;
use crate::{Capabilities, KeptReason, KeptSource, ListSink};

fn removal_of(plan: &OperationPlan, fs: &FakeFs, path: &str) -> u32 {
    let at = fs.location(path);
    (0_u32..)
        .zip(&plan.steps)
        .find(|(_, step)| matches!(step, Step::RemoveFile { location } if *location == at))
        .map(|(seq, _)| seq)
        .unwrap()
}

#[test]
fn a_move_goes_on_past_a_kept_source_and_reports_it() {
    for workers in MODES {
        let fs = fixture(&[("src/b/c.txt", BIG), ("src/b/d.txt", BIG)]);
        let plan = move_plan(&fs);
        let replace = || {
            fs.remove("src/b/d.txt");
            fs.add_file("src/b/d.txt", REPLACEMENT);
        };
        let at = fs.location("src/b/d.txt");
        let (outcome, harness) = {
            let mut harness = Harness {
                workers,
                ..Harness::default()
            };
            let fired = std::sync::atomic::AtomicBool::new(false);
            let hook = |location: &Location| {
                if *location == at && !fired.swap(true, std::sync::atomic::Ordering::SeqCst) {
                    replace();
                }
            };
            let ops = super::delete_races::BeforeRemove {
                fs: &fs,
                hook: &hook,
            };
            (harness.run(&ops, &plan, &[]), harness)
        };
        let seq = removal_of(&plan, &fs, "src/b/d.txt");
        assert!(
            !exists(&fs, "src/b/c.txt"),
            "workers {workers}: the move stopped at the kept source ({outcome:?})"
        );
        assert!(exists(&fs, "dst/b/c.txt") && exists(&fs, "dst/b/d.txt"));
        assert!(tree_of(&fs, "src").contains(&entry("b/d.txt", EntryKind::File, REPLACEMENT)));
        assert_eq!(
            failure(&outcome),
            Some((seq, FsErrorKind::ChangedOnDisk)),
            "workers {workers}"
        );
        let kept = KeptSource {
            seq,
            location: at.clone(),
            reason: KeptReason::Changed,
        };
        assert_eq!(harness.log.kept, vec![kept], "workers {workers}");
    }
}

#[test]
fn a_resumed_move_reports_its_kept_source_as_resumed() {
    for workers in MODES {
        let fs = fixture(&[("src/b/c.txt", BIG)]);
        let plan = move_plan(&fs);
        fs.add_folder("dst/b");
        fs.add_file("dst/b/c.txt", BIG);
        let done: Vec<u32> = (0_u32..)
            .zip(&plan.steps)
            .filter(|(_, step)| matches!(step, Step::MakeFolder { .. } | Step::CopyFile { .. }))
            .map(|(seq, _)| seq)
            .collect();
        let mut harness = Harness {
            workers,
            ..Harness::default()
        };
        let outcome = harness.run(&fs, &plan, &done);
        assert!(exists(&fs, "src/b/c.txt"), "workers {workers}: {outcome:?}");
        let reasons: Vec<_> = harness.log.kept.iter().map(|kept| &kept.reason).collect();
        assert_eq!(reasons, [&KeptReason::Resumed], "workers {workers}");
    }
}

#[test]
fn a_by_path_move_reports_a_source_swapped_during_its_copy_as_changed() {
    for workers in MODES {
        let fs = fixture(&[("src/b/c.txt", BIG)]);
        let plan = move_plan(&fs);
        let ops = SwapAfterCopy {
            fs: &fs,
            at: fs.location("src/b/c.txt"),
        };
        let (outcome, harness) = run_in(workers, &ops, &plan);
        assert!(
            tree_of(&fs, "src").contains(&entry("b/c.txt", EntryKind::File, REPLACEMENT)),
            "workers {workers}: the swapped-in file was deleted ({outcome:?})"
        );
        let kind = failure(&outcome).map(|(_, kind)| kind);
        assert_eq!(kind, Some(FsErrorKind::ChangedOnDisk), "workers {workers}");
        let reasons: Vec<_> = harness.log.kept.iter().map(|kept| &kept.reason).collect();
        assert_eq!(reasons, [&KeptReason::Changed], "workers {workers}");
    }
}

#[test]
fn a_move_still_stops_at_a_failed_copy() {
    for workers in MODES {
        let fs = fixture(&[("src/b/c.txt", BIG)]);
        fs.fault(
            "src/b/c.txt",
            FakeOp::Copy,
            1,
            Fault::Fail(FsErrorKind::DiskFull),
        );
        let plan = move_plan(&fs);
        let (outcome, harness) = run_in(workers, &fs, &plan);
        assert_eq!(
            failure(&outcome).map(|(_, kind)| kind),
            Some(FsErrorKind::DiskFull),
            "workers {workers}"
        );
        assert!(harness.log.kept.is_empty() && exists(&fs, "src/b/c.txt"));
    }
}

#[test]
fn a_move_skips_the_folder_of_a_kept_source() {
    for workers in MODES {
        let fs = fixture(&[("src/b/c.txt", BIG)]);
        let plan = move_plan(&fs);
        let replace = || {
            fs.remove("src/b/c.txt");
            fs.add_file("src/b/c.txt", REPLACEMENT);
        };
        let outcome = run_changing(workers, &fs, (&plan, "src/b/c.txt"), &replace);
        let seq = removal_of(&plan, &fs, "src/b/c.txt");
        assert_eq!(
            failure(&outcome),
            Some((seq, FsErrorKind::ChangedOnDisk)),
            "workers {workers}: the folder removal ran past the kept source"
        );
    }
}

#[test]
fn a_streamed_move_is_refused_before_any_step() {
    for workers in MODES {
        let fs = fixture(&[]);
        let source = Location::new("mem:///far/a.bin").unwrap();
        let plan = OperationPlan {
            kind: OpKind::Move,
            steps: vec![
                Step::MakeFolder {
                    target: fs.location("dst/sub"),
                },
                Step::CopyFile {
                    source: source.clone(),
                    target: fs.location("dst/sub/a.bin"),
                    size: 3,
                },
                Step::RemoveFile { location: source },
            ],
            totals: Totals {
                files: 1,
                folders: 1,
                bytes: 3,
            },
            conflicts: Vec::new(),
            risks: Vec::new(),
        };
        let (outcome, harness) = run_in(workers, &fs, &plan);
        assert_eq!(
            failure(&outcome),
            Some((1, FsErrorKind::NotSupportedYet)),
            "workers {workers}"
        );
        assert!(!exists(&fs, "dst/sub") && harness.logged().is_empty());
    }
}

/// A provider without handles: its move proof comes from the default by-path check. Swaps `at`
/// for another file once, right after its copy, and leaves the swap in place.
struct SwapAfterCopy<'a> {
    fs: &'a FakeFs,
    at: Location,
}

impl Provider for SwapAfterCopy<'_> {
    fn id(&self) -> ProviderId {
        self.fs.id()
    }

    fn capabilities(&self) -> Capabilities {
        self.fs.capabilities()
    }

    fn list(
        &self,
        folder: &Location,
        sink: &mut dyn ListSink,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        self.fs.list(folder, sink, cancel)
    }

    fn metadata(&self, location: &Location) -> Result<FsEntry, FsError> {
        self.fs.metadata(location)
    }
}

impl ProviderOps for SwapAfterCopy<'_> {
    fn create_folder(&self, location: &Location) -> Result<(), FsError> {
        self.fs.create_folder(location)
    }

    fn create_file(&self, location: &Location) -> Result<(), FsError> {
        self.fs.create_file(location)
    }

    fn copy_file(
        &self,
        source: &Location,
        target: &Location,
        progress: &mut dyn FnMut(u64),
        cancel: &CancelToken,
    ) -> Result<u64, FsError> {
        let written = self.fs.copy_file(source, target, progress, cancel)?;
        if *source == self.at && self.fs.metadata(source)?.size != REPLACEMENT {
            let path = "src/b/c.txt";
            self.fs.remove(path);
            self.fs.add_file(path, REPLACEMENT);
        }
        Ok(written)
    }

    fn file_identity(&self, location: &Location) -> Result<Option<crate::FileIdentity>, FsError> {
        self.fs.file_identity(location)
    }

    fn rename(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.fs.rename(source, target)
    }

    fn replace(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.fs.replace(source, target)
    }

    fn remove_file(&self, location: &Location) -> Result<(), FsError> {
        self.fs.remove_file(location)
    }

    fn remove_link(&self, location: &Location) -> Result<(), FsError> {
        self.fs.remove_link(location)
    }

    fn remove_empty_folder(&self, location: &Location) -> Result<(), FsError> {
        self.fs.remove_empty_folder(location)
    }

    fn recycle(&self, location: &Location) -> Result<(), FsError> {
        self.fs.recycle(location)
    }
}
