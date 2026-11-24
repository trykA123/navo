//! A move is a copy and then a removal; another program may change a source between the two.
//! These tests change the source right before the job removes it.

use std::sync::atomic::{AtomicBool, Ordering};

use navo_core::{Location, ProviderId};

use super::delete_races::BeforeRemove;
use super::*;
use crate::{Capabilities, CopyControl, ListSink, MoveCopy, OwnedCopy};

pub(super) const REPLACEMENT: u64 = 777;

pub(super) fn move_plan(fs: &FakeFs) -> OperationPlan {
    plan(&request(fs, OpKind::Move, &["src/b"], "dst"))
}

/// Runs `plan` and calls `change` once, just before the job first removes `at`.
pub(super) fn run_changing(
    workers: usize,
    fs: &FakeFs,
    (plan, at): (&OperationPlan, &str),
    change: &(dyn Fn() + Sync),
) -> JobOutcome {
    let (at, fired) = (fs.location(at), AtomicBool::new(false));
    let hook = |location: &Location| {
        if *location == at && !fired.swap(true, Ordering::SeqCst) {
            change();
        }
    };
    let ops = BeforeRemove { fs, hook: &hook };
    run_in(workers, &ops, plan).0
}

#[test]
fn a_move_keeps_a_source_file_replaced_after_its_copy() {
    for workers in MODES {
        let fs = fixture(&[("src/b/c.txt", BIG)]);
        let plan = move_plan(&fs);
        let replace = || {
            fs.remove("src/b/c.txt");
            fs.add_file("src/b/c.txt", REPLACEMENT);
        };
        let outcome = run_changing(workers, &fs, (&plan, "src/b/c.txt"), &replace);
        let kept = tree_of(&fs, "src").contains(&entry("b/c.txt", EntryKind::File, REPLACEMENT));
        assert!(
            kept,
            "workers {workers}: the replacement was never copied but the move deleted it \
             ({outcome:?}); dst holds {:?}",
            tree_of(&fs, "dst")
        );
    }
}

#[test]
fn a_move_stops_at_a_replaced_source_as_changed_on_disk() {
    for workers in MODES {
        let fs = fixture(&[("src/b/c.txt", BIG)]);
        let plan = move_plan(&fs);
        let replace = || {
            fs.remove("src/b/c.txt");
            fs.add_file("src/b/c.txt", REPLACEMENT);
        };
        let outcome = run_changing(workers, &fs, (&plan, "src/b/c.txt"), &replace);
        let kind = failure(&outcome).map(|(_, kind)| kind);
        assert_eq!(kind, Some(FsErrorKind::ChangedOnDisk), "workers {workers}");
    }
}

#[test]
fn a_move_keeps_a_same_size_same_time_replacement_by_its_file_id() {
    for workers in MODES {
        let fs = fixture(&[("src/b/c.txt", BIG)]);
        let plan = move_plan(&fs);
        let twin = || {
            fs.remove("src/b/c.txt");
            let twin = FsEntry {
                name: navo_core::ProviderPath::from_text("c.txt"),
                kind: EntryKind::File,
                size: BIG,
                modified_ms: Some(1_000),
                attributes: Attributes::default(),
            };
            fs.add_entry("src/b", twin);
        };
        let outcome = run_changing(workers, &fs, (&plan, "src/b/c.txt"), &twin);
        assert!(exists(&fs, "src/b/c.txt"), "workers {workers}: {outcome:?}");
        assert_eq!(
            failure(&outcome).map(|(_, kind)| kind),
            Some(FsErrorKind::ChangedOnDisk)
        );
    }
}

#[test]
fn a_same_volume_move_renames_without_a_copy() {
    for workers in MODES {
        let fs = fixture(&[("src/b/c.txt", BIG)]);
        let plan = plan(&PlanRequest {
            same_volume: true,
            ..request(&fs, OpKind::Move, &["src/b"], "dst")
        });
        assert!(
            plan.steps
                .iter()
                .all(|step| !matches!(step, Step::CopyFile { .. } | Step::RemoveFile { .. }))
        );
        let never: &(dyn Fn() + Sync) = &|| panic!("a rename removes nothing");
        let outcome = run_changing(workers, &fs, (&plan, "src/b/c.txt"), never);
        assert_eq!(outcome, JobOutcome::Done, "workers {workers}");
        assert!(exists(&fs, "dst/b/c.txt") && !exists(&fs, "src/b/c.txt"));
    }
}

#[test]
fn a_move_keeps_a_file_added_to_a_source_folder_after_its_listing() {
    for workers in MODES {
        let fs = fixture(&[("src/b/c.txt", BIG)]);
        let plan = move_plan(&fs);
        let add = || fs.add_file("src/b/late.txt", REPLACEMENT);
        let outcome = run_changing(workers, &fs, (&plan, "src/b"), &add);
        assert!(
            exists(&fs, "src/b/late.txt"),
            "workers {workers}: a file added after the listing was deleted ({outcome:?})"
        );
    }
}

#[test]
fn a_move_keeps_a_source_on_a_volume_without_file_ids() {
    for workers in MODES {
        let fs = fixture(&[("src/b/c.txt", BIG)]);
        fs.set_volume_serial("src", None);
        let plan = move_plan(&fs);
        let (outcome, _) = run_in(workers, &fs, &plan);
        assert!(
            exists(&fs, "src/b/c.txt"),
            "workers {workers}: a source without proof was deleted ({outcome:?})"
        );
        let kind = failure(&outcome).map(|(_, kind)| kind);
        assert_eq!(
            kind,
            Some(FsErrorKind::IdentityUnavailable),
            "workers {workers}"
        );
    }
}

#[test]
fn a_move_resumed_after_its_copy_keeps_the_source() {
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
        assert!(
            exists(&fs, "src/b/c.txt"),
            "workers {workers}: a resumed move deleted a source it never proved ({outcome:?})"
        );
        let kind = failure(&outcome).map(|(_, kind)| kind);
        assert_eq!(
            kind,
            Some(FsErrorKind::IdentityUnavailable),
            "workers {workers}"
        );
    }
}

#[test]
fn a_move_keeps_a_source_swapped_away_while_its_copy_opened() {
    for workers in MODES {
        let fs = fixture(&[("src/b/c.txt", BIG)]);
        let plan = move_plan(&fs);
        let swap = || {
            fs.rename_entry("src/b/c.txt", "src/b/c.aside");
            fs.add_file("src/b/c.txt", BIG);
        };
        let restore = || {
            fs.remove("src/b/c.txt");
            fs.rename_entry("src/b/c.aside", "src/b/c.txt");
        };
        let ops = SwapAroundCopy {
            fs: &fs,
            at: fs.location("src/b/c.txt"),
            fired: AtomicBool::new(false),
            swap: &swap,
            restore: &restore,
        };
        let (outcome, _) = run_in(workers, &ops, &plan);
        assert!(
            exists(&fs, "src/b/c.txt"),
            "workers {workers}: the move deleted a source its copy never read ({outcome:?})"
        );
        let kind = failure(&outcome).map(|(_, kind)| kind);
        assert_eq!(kind, Some(FsErrorKind::ChangedOnDisk), "workers {workers}");
    }
}

/// Swaps `at` for another file once, just before its copy opens it, and puts it back after.
struct SwapAroundCopy<'a> {
    fs: &'a FakeFs,
    at: Location,
    fired: AtomicBool,
    swap: &'a (dyn Fn() + Sync),
    restore: &'a (dyn Fn() + Sync),
}

impl SwapAroundCopy<'_> {
    fn around<T>(&self, source: &Location, copy: impl FnOnce() -> T) -> T {
        let fire = *source == self.at && !self.fired.swap(true, Ordering::SeqCst);
        if fire {
            (self.swap)();
        }
        let copied = copy();
        if fire {
            (self.restore)();
        }
        copied
    }
}

impl Provider for SwapAroundCopy<'_> {
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

impl ProviderOps for SwapAroundCopy<'_> {
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
        self.around(source, || {
            self.fs.copy_file(source, target, progress, cancel)
        })
    }

    fn copy_file_owned(
        &self,
        request: OwnedCopy<'_>,
        control: CopyControl<'_>,
    ) -> Result<u64, FsError> {
        let source = request.source;
        self.around(source, || self.fs.copy_file_owned(request, control))
    }

    fn copy_moved_file(
        &self,
        request: MoveCopy<'_>,
        control: CopyControl<'_>,
    ) -> Result<(u64, Option<crate::CopiedSource>), FsError> {
        let source = request.source;
        self.around(source, || self.fs.copy_moved_file(request, control))
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

    fn remove_copied_source(
        &self,
        location: &Location,
        expected: crate::CopiedSource,
    ) -> Result<(), FsError> {
        self.fs.remove_copied_source(location, expected)
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
