use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

struct Immutable {
    memory: Memory,
    metadata_calls: AtomicUsize,
    changes: bool,
}

impl SourceOps for Immutable {
    fn id(&self) -> ProviderId {
        memory_id()
    }

    fn metadata(&self, _: &Location) -> Result<SourceMetadata, FsError> {
        let calls = self.metadata_calls.fetch_add(1, Ordering::Relaxed);
        let id = u8::from(self.changes && calls > 0);
        Ok(SourceMetadata {
            kind: EntryKind::File,
            identity: SourceIdentity::Object(navo_git::ObjectId::Sha1([id; 20])),
        })
    }

    fn open_read(&self, location: &Location) -> Result<Box<dyn Read + Send>, FsError> {
        self.memory.open_read(location)
    }
}

fn scene(feed: Feed, estimate: u64, old: Option<&[u8]>, changes: bool) -> Scene {
    let mut scene = Scene::new(feed, old);
    let Step::CopyFile { size, .. } = &mut scene.plan.steps[0] else {
        unreachable!();
    };
    *size = estimate;
    scene.plan.totals.bytes = estimate;
    scene.sources = Sources::new().with(Arc::new(Immutable {
        memory: Memory {
            bytes: pattern(),
            feed,
            cancel: scene.cancel.clone(),
        },
        metadata_calls: AtomicUsize::new(0),
        changes,
    }));
    scene
}

#[test]
fn immutable_estimates_adjust_progress_to_streamed_bytes() {
    for (estimate, old) in [
        (0, None),
        ((SIZE - 1) as u64, None),
        (SIZE as u64, None),
        ((SIZE + 1) as u64, None),
        ((SIZE - 1) as u64, Some(OLD)),
        ((SIZE + 1) as u64, Some(OLD)),
    ] {
        for workers in [0, 1, 4] {
            let scene = scene(Feed::Whole, estimate, old, false);
            let mut harness = Harness::default();
            assert_eq!(scene.run(&mut harness, workers), JobOutcome::Done);
            assert_eq!(scene.target(), pattern());
            release_held(&harness.log.held, &LocalFs::new()).unwrap();
            assert_eq!(scene.names(), ["a.bin"]);
            assert_progress(&harness.progress, SIZE as u64, 1);
            for update in harness.progress.iter().filter(|p| p.files_done == 0) {
                assert_eq!(update.bytes_total, estimate.max(update.bytes_done));
            }
        }
    }
}

#[test]
fn several_immutable_copies_count_actual_bytes_for_each_estimate() {
    for workers in [0, 1, 4] {
        let mut scene = scene(Feed::Whole, (SIZE - CHUNK) as u64, None, false);
        for (name, estimate) in [("b.bin", (SIZE + CHUNK) as u64), ("c.bin", 0)] {
            let mut step = scene.plan.steps[0].clone();
            let Step::CopyFile { target, size, .. } = &mut step else {
                unreachable!();
            };
            *target = local(&scene.dir.path().join(name));
            *size = estimate;
            scene.plan.steps.push(step);
            scene.plan.totals.files += 1;
            scene.plan.totals.bytes += estimate;
        }
        let mut harness = Harness::default();
        assert_eq!(scene.run(&mut harness, workers), JobOutcome::Done);
        assert_progress(&harness.progress, 3 * SIZE as u64, 3);
        assert_eq!(scene.names(), ["a.bin", "b.bin", "c.bin"]);
        for name in scene.names() {
            assert_eq!(
                std::fs::read(scene.dir.path().join(name)).unwrap(),
                pattern()
            );
        }
    }
}

fn assert_progress(progress: &[Progress], bytes: u64, files: u64) {
    assert!(progress.iter().all(|p| p.bytes_done <= p.bytes_total));
    assert!(
        progress
            .windows(2)
            .all(|p| p[0].bytes_done <= p[1].bytes_done)
    );
    assert!(
        progress
            .iter()
            .any(|p| p.bytes_done > 0 && p.bytes_done < bytes)
    );
    let last = progress.last().unwrap();
    assert_eq!(last.bytes_done, bytes);
    assert_eq!(last.bytes_total, bytes);
    assert_eq!(last.files_done, files);
}

#[test]
fn an_immutable_source_with_a_different_object_id_is_refused() {
    for workers in MODES {
        let scene = scene(Feed::Whole, SIZE as u64, Some(OLD), true);
        let mut harness = Harness::default();
        assert_eq!(
            refused(&scene.run(&mut harness, workers)),
            FsErrorKind::ChangedOnDisk
        );
        assert_eq!(scene.target(), OLD);
        assert_eq!(scene.names(), ["a.bin"]);
        assert!(harness.log.held.is_empty());
    }
}

#[test]
fn cancelling_an_immutable_source_mid_file_leaves_no_partial_target() {
    for workers in MODES {
        for old in [None, Some(OLD)] {
            let scene = scene(Feed::CancelsAt(2 * CHUNK), 1, old, false);
            let mut harness = Harness::default();
            assert_eq!(scene.run(&mut harness, workers), JobOutcome::Cancelled);
            assert!(
                harness
                    .progress
                    .iter()
                    .any(|p| p.bytes_done > 0 && p.bytes_done < SIZE as u64)
            );
            if let Some(old) = old {
                assert_eq!(scene.target(), old);
                assert_eq!(scene.names(), ["a.bin"]);
            } else {
                assert!(scene.names().is_empty());
            }
            assert!(harness.log.held.is_empty());
        }
    }
}
