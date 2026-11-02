use super::*;

#[test]
fn a_copy_from_another_provider_writes_equal_bytes_and_reports_progress() {
    for workers in MODES {
        let scene = Scene::new(Feed::Whole, None);
        let mut harness = Harness::default();
        assert_eq!(scene.run(&mut harness, workers), JobOutcome::Done);
        assert_eq!(scene.target(), pattern());
        assert_eq!(scene.names(), ["a.bin"]);
        let bytes: Vec<u64> = harness.progress.iter().map(|p| p.bytes_done).collect();
        assert!(bytes.windows(2).all(|pair| pair[0] <= pair[1]), "{bytes:?}");
        assert!(bytes.iter().any(|&done| done > 0 && done < SIZE as u64));
        assert_eq!(bytes.last(), Some(&(SIZE as u64)));
        assert_eq!(harness.log.placed.len(), 1);
    }
}

#[test]
fn empty_and_chunk_boundary_files_copy_exactly() {
    for size in [0, CHUNK - 1, CHUNK, CHUNK + 1] {
        for workers in [0, 1, 4] {
            let bytes = vec![255; size];
            let scene = Scene::with_bytes(Feed::Whole, None, bytes.clone());
            let mut harness = Harness::default();
            assert_eq!(scene.run(&mut harness, workers), JobOutcome::Done);
            assert_eq!(scene.target(), bytes);
            assert_eq!(scene.names(), ["a.bin"]);
            assert_eq!(harness.progress.last().unwrap().files_done, 1);
            assert_eq!(harness.progress.last().unwrap().bytes_done, size as u64);
        }
    }
}

#[test]
fn an_existing_target_without_overwrite_permission_is_skipped() {
    for workers in MODES {
        let mut scene = Scene::new(Feed::OpenError, Some(OLD));
        scene.plan.conflicts.clear();
        let mut harness = Harness::default();
        assert_eq!(scene.run(&mut harness, workers), JobOutcome::Done);
        assert_eq!(scene.target(), OLD);
        assert_eq!(harness.logged(), [(0, StepOutcome::Skipped)]);
        assert_eq!(scene.names(), ["a.bin"]);
    }
}

#[test]
fn finished_steps_resume_without_opening_the_provider() {
    for workers in MODES {
        let scene = Scene::new(Feed::OpenError, Some(OLD));
        let mut harness = Harness::default();
        let outcome = harness.with(&LocalFs::new(), |ctx| match workers {
            0 => run_job_with_sources(&scene.plan, &[0], ctx, &scene.sources),
            n => run_job_parallel_with_sources((&scene.plan, &scene.sources), &[0], ctx, n),
        });
        assert_eq!(outcome, JobOutcome::Done);
        assert_eq!(scene.target(), OLD);
        assert!(harness.logged().is_empty());
        assert_eq!(scene.names(), ["a.bin"]);
    }
}

#[test]
fn one_plan_copies_from_a_foreign_provider_and_the_local_disk() {
    for workers in MODES {
        let mut scene = Scene::new(Feed::Whole, None);
        let local_source = scene.dir.path().join("source.bin");
        std::fs::write(&local_source, OLD).unwrap();
        scene.plan.steps.push(Step::CopyFile {
            source: local(&local_source),
            target: local(&scene.dir.path().join("local.bin")),
            size: OLD.len() as u64,
        });
        scene.plan.totals.files += 1;
        scene.plan.totals.bytes += OLD.len() as u64;
        let mut harness = Harness::default();
        assert_eq!(scene.run(&mut harness, workers), JobOutcome::Done);
        assert_eq!(scene.target(), pattern());
        assert_eq!(
            std::fs::read(scene.dir.path().join("local.bin")).unwrap(),
            OLD
        );
        assert_eq!(std::fs::read(local_source).unwrap(), OLD);
        assert_eq!(harness.progress.last().unwrap().files_done, 2);
        assert_eq!(scene.names(), ["a.bin", "local.bin", "source.bin"]);
    }
}

#[test]
fn a_cancel_mid_file_leaves_no_target_and_no_temporary() {
    for workers in MODES {
        let scene = Scene::new(Feed::CancelsAt(2 * CHUNK), None);
        let mut harness = Harness::default();
        assert_eq!(scene.run(&mut harness, workers), JobOutcome::Cancelled);
        assert!(scene.names().is_empty(), "{:?}", scene.names());
    }
}

#[test]
fn an_overwrite_from_another_provider_holds_the_old_file() {
    for workers in MODES {
        let scene = Scene::new(Feed::Whole, Some(OLD));
        let mut harness = Harness::default();
        assert_eq!(scene.run(&mut harness, workers), JobOutcome::Done);
        assert_eq!(scene.target(), pattern());
        let [held] = harness.log.held.as_slice() else {
            panic!("one held file");
        };
        assert_eq!(
            std::fs::read(crate::os_path(&held.held).unwrap()).unwrap(),
            OLD
        );
        release_held(&harness.log.held, &LocalFs::new()).unwrap();
        assert_eq!(scene.names(), ["a.bin"]);
    }
}

#[test]
fn a_source_that_ends_early_or_grows_is_refused() {
    for feed in [
        Feed::Short(1),
        Feed::Short(CHUNK + 5),
        Feed::Long(1),
        Feed::Long(CHUNK),
    ] {
        for workers in MODES {
            let scene = Scene::new(feed, None);
            let outcome = scene.run(&mut Harness::default(), workers);
            assert_eq!(refused(&outcome), FsErrorKind::ChangedOnDisk);
            assert!(scene.names().is_empty(), "{:?}", scene.names());
        }
    }
}

#[test]
fn a_bad_source_or_a_cancel_never_touches_the_old_target() {
    let feeds = [
        Feed::Short(3),
        Feed::Long(3),
        Feed::BreaksAt(2 * CHUNK),
        Feed::CancelsAt(2 * CHUNK),
    ];
    for feed in feeds {
        for workers in MODES {
            let scene = Scene::new(feed, Some(OLD));
            let mut harness = Harness::default();
            assert_ne!(scene.run(&mut harness, workers), JobOutcome::Done);
            assert_eq!(scene.target(), OLD);
            assert_eq!(scene.names(), ["a.bin"]);
            assert!(harness.log.held.is_empty());
        }
    }
}

#[test]
fn a_read_error_mid_stream_cleans_up() {
    for workers in MODES {
        let scene = Scene::new(Feed::BreaksAt(2 * CHUNK), None);
        let mut harness = Harness::default();
        let outcome = scene.run(&mut harness, workers);
        assert_ne!(refused(&outcome), FsErrorKind::Cancelled);
        assert!(scene.names().is_empty(), "{:?}", scene.names());
        assert!(harness.progress.iter().any(|p| p.bytes_done > 0));
    }
}

#[test]
fn a_source_whose_size_changed_since_planning_is_refused_before_writing() {
    for workers in MODES {
        let mut scene = Scene::new(Feed::Whole, None);
        if let Step::CopyFile { size, .. } = &mut scene.plan.steps[0] {
            *size -= 1;
        }
        let outcome = scene.run(&mut Harness::default(), workers);
        assert_eq!(refused(&outcome), FsErrorKind::ChangedOnDisk);
        assert!(scene.names().is_empty());
    }
}

#[test]
fn a_provider_without_a_reader_is_refused() {
    let mut scene = Scene::new(Feed::Whole, None);
    scene.sources = Sources::new();
    let outcome = scene.run(&mut Harness::default(), 0);
    assert_eq!(refused(&outcome), FsErrorKind::NotSupportedYet);
    assert!(scene.names().is_empty());
}
