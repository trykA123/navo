use navo_core::brand;
use navo_store::{Db, JobJournal};

use super::*;
use crate::jobs::{SharedJournal, lock, log_factory, recover};

fn journal() -> SharedJournal {
    Arc::new(Mutex::new(
        JobJournal::new(Db::open_in_memory().unwrap()).unwrap(),
    ))
}

/// Runs the paste with the journal and stops as a crash would: no finish is written.
fn crash_during(paste: &StreamPaste, target: &Path, journal: &SharedJournal) -> JobOutcome {
    let plan = plan_into(paste, &location(target));
    let mut log = log_factory(Arc::clone(journal))(&plan).unwrap();
    run_job_parallel_with_sources(
        (&plan, &paste.sources),
        &[],
        &mut JobContext {
            ops: &LocalFs::new(),
            log: log.as_mut(),
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
        },
        1,
    )
}

#[test]
fn an_interrupted_paste_removes_its_temp_files_and_never_reads_the_source_again() {
    let dir = tempfile::tempdir().unwrap();
    let journal = journal();
    let scene = remote(
        vec![
            file(0, "box\\done.txt", Some(4)),
            file(1, "box\\half.txt", Some(9)),
        ],
        vec![
            Feed::Bytes(b"done".to_vec(), Then::End),
            Feed::Bytes(b"half".to_vec(), Then::Fail("The session ended")),
        ],
    );
    let outcome = crash_during(&scene.paste, dir.path(), &journal);
    assert!(matches!(outcome, JobOutcome::Failed { .. }), "{outcome:?}");
    let opened = scene.opened.lock().unwrap().clone();
    drop(scene);
    let stray = dir
        .path()
        .join(format!("box/half.txt{}", brand::TEMP_SUFFIX));
    std::fs::write(&stray, b"half").unwrap();
    assert_eq!(recover(&journal, &LocalFs::new()).unwrap(), 1);
    assert!(lock(&journal).unfinished().unwrap().is_empty());
    assert_eq!(tree(dir.path()), ["box", "box/done.txt"]);
    assert_eq!(
        std::fs::read(dir.path().join("box/done.txt")).unwrap(),
        b"done"
    );
    assert_eq!(opened, [0, 1]);
}
