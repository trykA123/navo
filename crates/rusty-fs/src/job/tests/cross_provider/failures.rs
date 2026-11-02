use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

#[test]
fn an_interrupted_read_retries_without_losing_bytes() {
    for workers in MODES {
        let scene = Scene::new(Feed::InterruptsAt(CHUNK), None);
        assert_eq!(
            scene.run(&mut Harness::default(), workers),
            JobOutcome::Done
        );
        assert_eq!(scene.target(), pattern());
        assert_eq!(scene.names(), ["a.bin"]);
    }
}

struct Changing {
    metadata_calls: AtomicUsize,
    after: FileStamp,
}

impl SourceOps for Changing {
    fn id(&self) -> ProviderId {
        memory_id()
    }

    fn metadata(&self, location: &Location) -> Result<SourceMetadata, FsError> {
        let stamp = if self.metadata_calls.fetch_add(1, Ordering::Relaxed) == 0 {
            FileStamp {
                size: SIZE as u64,
                modified_ms: Some(1_000),
            }
        } else {
            self.after
        };
        Ok(FsEntry {
            name: location.name().unwrap(),
            kind: EntryKind::File,
            size: stamp.size,
            modified_ms: stamp.modified_ms,
            attributes: Attributes::default(),
        }
        .into())
    }

    fn open_read(&self, _: &Location) -> Result<Box<dyn Read + Send>, FsError> {
        Ok(Box::new(io::Cursor::new(pattern())))
    }
}

#[test]
fn a_source_changed_during_reading_preserves_the_old_target() {
    for workers in MODES {
        let mut scene = Scene::new(Feed::Whole, Some(OLD));
        scene.sources = Sources::new().with(Arc::new(Changing {
            metadata_calls: AtomicUsize::new(0),
            after: FileStamp {
                size: SIZE as u64,
                modified_ms: Some(2_000),
            },
        }));
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
fn a_live_source_whose_size_changes_during_reading_is_refused() {
    for workers in MODES {
        let mut scene = Scene::new(Feed::Whole, None);
        scene.sources = Sources::new().with(Arc::new(Changing {
            metadata_calls: AtomicUsize::new(0),
            after: FileStamp {
                size: SIZE as u64 + 1,
                modified_ms: Some(1_000),
            },
        }));
        assert_eq!(
            refused(&scene.run(&mut Harness::default(), workers)),
            FsErrorKind::ChangedOnDisk
        );
        assert!(scene.names().is_empty());
    }
}

#[test]
fn source_metadata_and_open_errors_preserve_the_target_and_identify_the_source() {
    for (feed, kind) in [
        (Feed::MetadataError, FsErrorKind::AccessDenied),
        (Feed::OpenError, FsErrorKind::Locked),
    ] {
        for workers in MODES {
            let scene = Scene::new(feed, Some(OLD));
            let JobOutcome::Failed { seq, error } = scene.run(&mut Harness::default(), workers)
            else {
                panic!("expected source failure");
            };
            assert_eq!(seq, 0);
            assert_eq!(error.kind, kind);
            assert_eq!(
                error.location,
                Some(Location::new("mem:///far/a.bin").unwrap())
            );
            assert_eq!(scene.target(), OLD);
            assert_eq!(scene.names(), ["a.bin"]);
        }
    }
}

struct Appearing {
    target: std::path::PathBuf,
    memory: Memory,
}

impl SourceOps for Appearing {
    fn id(&self) -> ProviderId {
        memory_id()
    }

    fn metadata(&self, location: &Location) -> Result<SourceMetadata, FsError> {
        self.memory.metadata(location)
    }

    fn open_read(&self, location: &Location) -> Result<Box<dyn Read + Send>, FsError> {
        std::fs::write(&self.target, OLD).unwrap();
        self.memory.open_read(location)
    }
}

#[test]
fn a_target_created_while_reading_is_preserved_and_the_temporary_is_removed() {
    for workers in MODES {
        let mut scene = Scene::new(Feed::Whole, None);
        scene.sources = Sources::new().with(Arc::new(Appearing {
            target: scene.dir.path().join("a.bin"),
            memory: Memory {
                bytes: pattern(),
                feed: Feed::Whole,
                cancel: scene.cancel.clone(),
            },
        }));
        let mut harness = Harness::default();
        assert_ne!(scene.run(&mut harness, workers), JobOutcome::Done);
        assert_eq!(scene.target(), OLD);
        assert_eq!(scene.names(), ["a.bin"]);
        assert!(harness.log.held.is_empty());
    }
}

#[test]
fn a_failed_temporary_create_reports_the_target_without_creating_any_file() {
    for workers in MODES {
        let mut scene = Scene::new(Feed::Whole, None);
        let Step::CopyFile { target, .. } = &mut scene.plan.steps[0] else {
            unreachable!();
        };
        *target = local(&scene.dir.path().join("missing").join("a.bin"));
        let temporary = crate::job::copy::temp_of(target).unwrap();
        let JobOutcome::Failed { error, .. } = scene.run(&mut Harness::default(), workers) else {
            panic!("expected creation failure");
        };
        assert_eq!(error.kind, FsErrorKind::NotFound);
        assert_eq!(error.location, Some(temporary));
        assert!(scene.names().is_empty());
    }
}
