use std::sync::mpsc;
use std::time::Instant;

use navo_core::AppState;
use navo_shell::BadName;
use navo_ui::browser::{Browser, JobStatus, Refused};

use super::*;

struct Rig {
    browser: Browser,
    awake: mpsc::Receiver<()>,
}

impl Rig {
    fn new(dir: &Path, operations: Operations) -> Self {
        let (wake, awake) = mpsc::channel();
        let mut browser = Browser::new(
            AppState::new(location(dir)),
            Arc::new(LocalFs::new()),
            Arc::new(move || {
                let _ = wake.send(());
            }),
        );
        browser.set_operations(operations);
        Self { browser, awake }
    }

    fn wait_until(&mut self, done: impl Fn(&Browser) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done(&self.browser) {
            self.browser.poll();
            if done(&self.browser) {
                break;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            self.awake
                .recv_timeout(left)
                .expect("the browser woke in time");
        }
    }

    fn finish(&mut self) -> JobStatus {
        self.wait_until(|browser| {
            browser
                .jobs()
                .first()
                .is_some_and(|job| !job.status.is_active())
        });
        self.browser.jobs()[0].status.clone()
    }
}

fn entries() -> Vec<VirtualEntry> {
    vec![
        file(0, "kept.txt", Some(3)),
        file(1, "new.txt", Some(3)),
        refused(2, "secret.txt:hidden", BadName::Stream),
    ]
}

fn feeds() -> Vec<Feed> {
    vec![
        Feed::Bytes(b"NEW".to_vec(), Then::End),
        Feed::Bytes(b"new".to_vec(), Then::End),
        Feed::Bytes(b"bad".to_vec(), Then::End),
    ]
}

#[test]
fn a_name_already_there_is_asked_before_any_byte_is_read() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("kept.txt"), b"old").unwrap();
    let scene = remote(entries(), feeds());
    let mut rig = Rig::new(dir.path(), Operations::unlogged());
    rig.browser
        .paste_stream(scene.paste, location(dir.path()))
        .unwrap();
    rig.wait_until(|browser| browser.pending().is_some());
    let pending = rig.browser.pending().unwrap();
    assert_eq!(pending.plan.conflicts.len(), 1);
    assert_eq!(
        pending.plan.kind,
        OpKind::Copy,
        "a paste of virtual files never moves"
    );
    assert!(
        scene.opened.lock().unwrap().is_empty(),
        "nothing is read before the answer"
    );
    assert!(rig.browser.confirm());
    assert_eq!(rig.finish(), JobStatus::Done);
    assert_eq!(
        *scene.opened.lock().unwrap(),
        [1],
        "the skipped item is never read"
    );
    assert_eq!(std::fs::read(dir.path().join("kept.txt")).unwrap(), b"old");
    assert_eq!(std::fs::read(dir.path().join("new.txt")).unwrap(), b"new");
    assert_eq!(
        rig.browser.jobs()[0].refused,
        [Refused {
            name: "secret.txt:hidden".into(),
            reason: "The name points to an alternate data stream".into(),
        }]
    );
}

#[test]
fn without_conflicts_the_paste_starts_at_once_through_the_profile_guard() {
    let dir = tempfile::tempdir().unwrap();
    let scene = remote(entries(), feeds());
    let operations = Operations {
        log: crate::jobs::guard_factory(None),
        ..Operations::unlogged()
    };
    let mut rig = Rig::new(dir.path(), operations);
    rig.browser
        .paste_stream(scene.paste, location(dir.path()))
        .unwrap();
    assert_eq!(rig.finish(), JobStatus::Done);
    assert!(rig.browser.pending().is_none());
    assert_eq!(tree(dir.path()), ["kept.txt", "new.txt"]);
}

#[test]
fn a_failed_paste_shows_the_source_reason() {
    let dir = tempfile::tempdir().unwrap();
    let scene = remote(
        vec![file(0, "lost.txt", Some(100))],
        vec![Feed::Bytes(vec![1; 10], Then::Fail("The session ended"))],
    );
    let mut rig = Rig::new(dir.path(), Operations::unlogged());
    rig.browser
        .paste_stream(scene.paste, location(dir.path()))
        .unwrap();
    let JobStatus::Failed(message) = rig.finish() else {
        panic!("the paste fails");
    };
    assert!(message.contains("The session ended"), "{message}");
    assert!(tree(dir.path()).is_empty());
}

#[test]
fn the_experimental_profile_journals_a_paste_with_a_stream_and_a_mail() {
    let profile = tempfile::tempdir().unwrap();
    let dirs = navo_store::StoreDirs::in_profile(profile.path());
    dirs.ensure().unwrap();
    let journal = crate::jobs::open(&dirs.journal_file()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let scene = scene(
        vec![file(0, "note.txt", Some(2)), file(1, "mail.msg", None)],
        vec![
            Feed::Bytes(b"hi".to_vec(), Then::End),
            Feed::Storage(b"msg".to_vec()),
        ],
        (SourceKind::Mail, Duration::from_secs(5)),
    );
    let operations = Operations {
        log: crate::jobs::experimental_factory(Arc::clone(&journal), dirs),
        ..Operations::unlogged()
    };
    let mut rig = Rig::new(dir.path(), operations);
    rig.browser
        .paste_stream(scene.paste, location(dir.path()))
        .unwrap();
    assert_eq!(rig.finish(), JobStatus::Done);
    assert_eq!(tree(dir.path()), ["mail.msg", "note.txt"]);
    assert!(crate::jobs::lock(&journal).unfinished().unwrap().is_empty());
}
