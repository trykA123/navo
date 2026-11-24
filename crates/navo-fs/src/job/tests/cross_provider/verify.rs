//! Verify-after-copy for steps from another provider: the stream is hashed as it is written,
//! and a file the source program wrote itself counts as unverified.

use crate::job::copy::{Copier, CopyStep};
use crate::job::plan_targets::PlanTargets;
use crate::job::remove::Removals;
use crate::job::verify::{HashingRead, SourceDigest, check};
use crate::{CopyControl, StreamWrite};

use super::*;

/// A scene whose target folder lies under target/, not /tmp: tmpfs may refuse the cache-drop
/// hint, and then the uncached read counts as unverified.
fn on_disk(scene: Scene) -> Scene {
    let exe = std::env::current_exe().unwrap();
    let dir = tempfile::tempdir_in(exe.parent().unwrap()).unwrap();
    let mut scene = Scene { dir, ..scene };
    if let Some(Step::CopyFile { target, .. }) = scene.plan.steps.first_mut() {
        *target = local(&scene.dir.path().join("a.bin"));
    }
    scene
}

/// Runs `scene` with `workers` and the verify counter `count` switched on.
fn run_verified(scene: &Scene, workers: usize, count: &VerifyCount) -> JobOutcome {
    let mut harness = Harness {
        keeps_held: true,
        cancel: scene.cancel.clone(),
        ..Harness::default()
    };
    harness.with(&LocalFs::new(), |ctx| {
        let options = JobOptions {
            workers,
            sources: Some(&scene.sources),
            verify: Some(count),
            speed_cap: None,
        };
        run_job_with(&scene.plan, &[], ctx, options)
    })
}

#[test]
fn a_streamed_copy_is_hashed_in_the_stream_and_counts_verified() {
    for workers in MODES {
        let scene = on_disk(Scene::new(Feed::Whole, None));
        let count = VerifyCount::default();
        assert_eq!(run_verified(&scene, workers, &count), JobOutcome::Done);
        assert_eq!(scene.target(), pattern());
        assert_eq!(
            (count.verified(), count.unverified()),
            (1, 0),
            "the stream was not hashed, or the volume under {} refused the uncached read",
            scene.dir.path().display()
        );
        assert!(!count.verifying());
    }
}

#[test]
fn hashing_read_digests_exactly_the_bytes_it_returns() {
    let bytes = pattern();
    let mut reader = HashingRead::new(Chunks {
        bytes: bytes.clone(),
        at: 0,
        breaks_at: None,
        cancels: None,
        interrupts_at: Some(CHUNK),
    });
    let mut seen = Vec::new();
    let mut buf = [0; 1000];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => seen.extend_from_slice(&buf[..n]),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => panic!("{error}"),
        }
    }
    assert_eq!(seen, bytes);
    assert_eq!(reader.digest(), *blake3::hash(&bytes).as_bytes());
}

/// A source that writes the job's temporary file itself, as a source program does.
struct Writer;

impl SourceOps for Writer {
    fn id(&self) -> ProviderId {
        memory_id()
    }

    fn metadata(&self, location: &Location) -> Result<SourceMetadata, FsError> {
        Memory {
            bytes: pattern(),
            feed: Feed::Whole,
            cancel: CancelToken::new(),
        }
        .metadata(location)
    }

    fn open_read(&self, location: &Location) -> Result<Box<dyn Read + Send>, FsError> {
        Err(crate::job::copy::failure(
            FsErrorKind::NotSupportedYet,
            location,
        ))
    }

    fn open_into(
        &self,
        location: &Location,
        (temp, created): (&Location, TempCreated<'_>),
        cancel: &CancelToken,
    ) -> Result<SourceContent, FsError> {
        let bytes = pattern();
        LocalFs::new().write_new(
            StreamWrite {
                bytes: &mut bytes.as_slice(),
                source: location,
                target: temp,
                created: None,
                modified: None,
            },
            CopyControl {
                progress: &mut |_| {},
                cancel,
            },
        )?;
        created()?;
        Ok(SourceContent::Written)
    }
}

#[test]
fn a_file_the_source_wrote_itself_counts_unverified() {
    for workers in MODES {
        let mut scene = on_disk(Scene::new(Feed::Whole, None));
        scene.sources = Sources::new().with(Arc::new(Writer));
        let count = VerifyCount::default();
        assert_eq!(run_verified(&scene, workers, &count), JobOutcome::Done);
        assert_eq!(scene.target(), pattern());
        assert_eq!((count.verified(), count.unverified()), (0, 1));
    }
}

#[test]
fn a_streamed_digest_that_differs_from_the_target_fails_with_verify_failed() {
    let scene = on_disk(Scene::new(Feed::Whole, None));
    let path = scene.dir.path().join("a.bin.part");
    std::fs::write(&path, pattern()).unwrap();
    let temp = local(&path);
    let Some(Step::CopyFile { source, target, .. }) = scene.plan.steps.first() else {
        panic!("the scene holds one copy step");
    };
    let (removals, targets) = (
        Removals::of(&scene.plan, &LocalFs::new()),
        PlanTargets::of(&scene.plan),
    );
    let (ops, count) = (LocalFs::new(), VerifyCount::default());
    let copier = Copier {
        ops: &ops,
        cancel: &scene.cancel,
        strict: false,
        namespace: None,
        removals: &removals,
        sources: Some(&scene.sources),
        targets: &targets,
        verify: Some(&count),
        throttle: None,
    };
    let step = CopyStep {
        seq: 0,
        source,
        target,
        size: SIZE as u64,
    };
    let error = check(&copier, (&step, &temp), SourceDigest::Streamed([9; 32])).unwrap_err();
    assert_eq!(error.kind, FsErrorKind::VerifyFailed);
    assert_eq!((count.verified(), count.unverified()), (0, 0));
    assert!(!count.verifying());
}
