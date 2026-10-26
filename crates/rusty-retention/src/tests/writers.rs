use super::*;

fn fixture(fs: &FakeFs) -> Location {
    fs.add_folder("home/tree");
    fs.add_file("home/tree/a", 7);
    fs.add_file("home/tree/b", 9);
    fs.location("home/tree")
}

#[test]
fn a_paused_capture_leaves_the_database_available_to_another_writer() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    let source = fixture(&fs);
    let mut vault = open(profile.path(), &fs);
    let (writer, _) = Db::open(&profile.path().join("ferrite.db"), false).unwrap();
    writer
        .connection()
        .busy_timeout(std::time::Duration::from_millis(100))
        .unwrap();
    let cancel = CancelToken::new();
    let (paused, ready) = std::sync::mpsc::channel();
    let mut report = |progress: RetentionProgress| {
        if progress.visited != 4 {
            return;
        }
        cancel.pause();
        paused.send(()).unwrap();
        cancel.wait_while_paused();
    };
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            vault.capture(
                CaptureRequest {
                    source: &source,
                    policy: Default::default(),
                    created_at: 0,
                    job: None,
                },
                &mut RetentionRun::new(&cancel, &mut report),
            )
        });
        ready
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        let written = writer
            .connection()
            .execute("INSERT INTO tags VALUES ('/paused', 'writer')", []);
        cancel.resume();
        worker.join().unwrap().unwrap();
        assert!(written.is_ok(), "{written:?}");
    });
}

#[derive(Clone, Copy)]
enum Phase {
    Capture,
    Restore,
    Purge,
    Reconcile,
}

fn callbacks_can_write(phase: Phase) {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    let source = fixture(&fs);
    let mut vault = open(profile.path(), &fs);
    let entry = if matches!(phase, Phase::Capture) {
        None
    } else {
        Some(capture(&mut vault, &source))
    };
    if matches!(phase, Phase::Reconcile) {
        raw_state(profile.path(), entry.as_ref().unwrap().id, "preparing");
    }
    let (writer, _) = Db::open(&profile.path().join("ferrite.db"), false).unwrap();
    writer
        .connection()
        .busy_timeout(std::time::Duration::from_millis(10))
        .unwrap();
    let mut writes = Vec::new();
    let mut report = |_: RetentionProgress| {
        writes.push(writer.connection().execute(
            "INSERT OR IGNORE INTO tags VALUES ('/progress', 'writer')",
            [],
        ))
    };
    let cancel = CancelToken::new();
    let mut run = RetentionRun::new(&cancel, &mut report);
    exercise(&mut vault, &mut run, (phase, entry, &source));
    assert!(!writes.is_empty());
    assert!(writes.iter().all(Result::is_ok), "{writes:?}");
}

fn exercise(
    vault: &mut Vault<'_>,
    run: &mut RetentionRun<'_>,
    (phase, entry, source): (Phase, Option<RetentionEntry>, &Location),
) {
    match (phase, entry) {
        (Phase::Capture, _) => {
            vault
                .capture(
                    CaptureRequest {
                        source,
                        policy: Default::default(),
                        created_at: 0,
                        job: None,
                    },
                    run,
                )
                .unwrap();
        }
        (Phase::Restore, Some(entry)) => {
            vault
                .restore(
                    RestoreRequest {
                        id: entry.id,
                        target: &entry
                            .record
                            .source
                            .parent()
                            .unwrap()
                            .join(&ProviderPath::from_text("restored"))
                            .unwrap(),
                    },
                    run,
                )
                .unwrap();
        }
        (Phase::Purge, Some(entry)) => {
            vault.purge(entry.id, run).unwrap();
        }
        (Phase::Reconcile, Some(entry)) => {
            vault.reconcile(entry.id, run).unwrap();
        }
        _ => unreachable!(),
    }
}

#[test]
fn capture_callbacks_leave_writers_available() {
    callbacks_can_write(Phase::Capture);
}
#[test]
fn restore_callbacks_leave_writers_available() {
    callbacks_can_write(Phase::Restore);
}
#[test]
fn purge_callbacks_leave_writers_available() {
    callbacks_can_write(Phase::Purge);
}
#[test]
fn reconcile_callbacks_leave_writers_available() {
    callbacks_can_write(Phase::Reconcile);
}
