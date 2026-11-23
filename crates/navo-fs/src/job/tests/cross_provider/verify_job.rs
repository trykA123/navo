//! Whole-job tests for verify-after-copy of streamed sources: a copy corrupted before the target
//! re-read fails the job without leftovers, a grown source is refused before any hash, and a job
//! with verify off never compares.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use super::*;

/// A memory source that corrupts every file in `dir` except the target once streaming has ended:
/// the job reads the source metadata again after the write and before the verify re-read.
struct Corrupting {
    memory: Memory,
    dir: PathBuf,
    opened: AtomicBool,
}

impl Corrupting {
    /// Flips the first byte of every file in the folder except `a.bin`; the size stays the same.
    fn corrupt(&self) {
        for entry in std::fs::read_dir(&self.dir).unwrap() {
            let path = entry.unwrap().path();
            if path.file_name().is_some_and(|name| name == "a.bin") {
                continue;
            }
            let mut bytes = std::fs::read(&path).unwrap();
            bytes[0] ^= 0xFF;
            std::fs::write(&path, bytes).unwrap();
        }
    }
}

impl SourceOps for Corrupting {
    fn id(&self) -> ProviderId {
        memory_id()
    }

    fn metadata(&self, location: &Location) -> Result<SourceMetadata, FsError> {
        if self.opened.swap(false, Ordering::SeqCst) {
            self.corrupt();
        }
        self.memory.metadata(location)
    }

    fn open_read(&self, location: &Location) -> Result<Box<dyn Read + Send>, FsError> {
        self.opened.store(true, Ordering::SeqCst);
        self.memory.open_read(location)
    }
}

/// A plain copy scene whose source corrupts the temporary file after streaming.
fn corrupting_scene() -> Scene {
    let mut scene = Scene::new(Feed::Whole, None);
    let corrupting = Corrupting {
        memory: Memory {
            bytes: pattern(),
            feed: Feed::Whole,
            cancel: scene.cancel.clone(),
        },
        dir: scene.dir.path().to_path_buf(),
        opened: AtomicBool::new(false),
    };
    scene.sources = Sources::new().with(Arc::new(corrupting));
    scene
}

/// Runs `scene` with `workers`; `count` switches verify on, `None` leaves it off.
fn run_with(scene: &Scene, workers: usize, count: Option<&VerifyCount>) -> (JobOutcome, Harness) {
    let mut harness = Harness {
        keeps_held: true,
        cancel: scene.cancel.clone(),
        ..Harness::default()
    };
    let outcome = harness.with(&LocalFs::new(), |ctx| {
        let options = JobOptions {
            workers,
            sources: Some(&scene.sources),
            verify: count,
        };
        run_job_with(&scene.plan, &[], ctx, options)
    });
    (outcome, harness)
}

#[test]
fn a_streamed_copy_corrupted_before_the_reread_fails_and_leaves_no_temporary() {
    for workers in MODES {
        let scene = corrupting_scene();
        let count = VerifyCount::default();
        let (outcome, _) = run_with(&scene, workers, Some(&count));
        assert_eq!(failure(&outcome), Some((0, FsErrorKind::VerifyFailed)));
        assert_eq!(scene.names(), Vec::<String>::new());
        assert!(!scene.dir.path().join("a.bin").exists());
        assert_eq!((count.verified(), count.unverified()), (0, 0));
        assert!(!count.verifying());
    }
}

#[test]
fn a_grown_source_with_verify_on_ends_changed_on_disk_before_any_hash() {
    for workers in MODES {
        let scene = Scene::new(Feed::Long(10), None);
        let count = VerifyCount::default();
        let (outcome, _) = run_with(&scene, workers, Some(&count));
        assert_eq!(refused(&outcome), FsErrorKind::ChangedOnDisk);
        assert_eq!((count.verified(), count.unverified()), (0, 0));
        assert_eq!(scene.names(), Vec::<String>::new());
    }
}

/// Proves verify off never compares: a temporary file corrupted after streaming is placed as is.
/// It cannot show that no `HashingRead` was built, because hashing has no outside effect.
#[test]
fn a_streamed_copy_with_verify_off_places_the_file_without_comparing() {
    for workers in MODES {
        let scene = corrupting_scene();
        let (outcome, _) = run_with(&scene, workers, None);
        assert_eq!(outcome, JobOutcome::Done);
        let mut corrupted = pattern();
        corrupted[0] ^= 0xFF;
        assert_eq!(scene.target(), corrupted);
    }
}
