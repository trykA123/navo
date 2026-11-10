//! Worker tests: retained sessions are released on Close, recovery without one fails closed, scans fail closed.

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use crate::flow::{Event, Failure, InstallAction, Request, SetupExit, Shortcuts};
use crate::mode::{Discovery, Mode};
use crate::payload::Payload;
use crate::placement::Space;
use crate::runner::Registry;
use crate::runner_fake::{Call, FakeHost, MarkerWrite};
use crate::syntax::{Candidate, FolderProblem};
use crate::window_tests::{ROOT, VERSION, candidate, payload};
use crate::window_worker::{Job, Probe, Reply, Worker};

/// A probe that passes every folder and records launches.
#[derive(Default)]
struct FakeProbe {
    started: Vec<PathBuf>,
}

impl Probe for FakeProbe {
    fn check_folder(
        &mut self,
        _: &Candidate,
        _: &Space,
    ) -> (Result<(), FolderProblem>, Option<u64>) {
        (Ok(()), Some(1))
    }
    fn start_navo(&mut self, root: &Path) -> std::io::Result<()> {
        self.started.push(root.to_owned());
        Ok(())
    }
}

/// Runs `jobs` through a worker on this thread and returns its replies.
fn serve(host: &mut FakeHost, payload: &Payload, jobs: Vec<Job>) -> Vec<Reply> {
    let (send, inbox) = mpsc::channel();
    for job in jobs {
        send.send(job).unwrap();
    }
    drop(send);
    let mut probe = FakeProbe::default();
    let log = Path::new(r"C:\Temp\velopack.log");
    let mut worker = Worker::new(
        host,
        &mut probe,
        Some(payload),
        log,
        PathBuf::from(r"C:\Temp\navo-setup.log"),
    );
    let mut replies = Vec::new();
    worker.serve(&inbox, &mut |reply| replies.push(reply));
    replies
}

fn request(action: InstallAction) -> Request {
    Request {
        mode: Box::new(Mode::Fresh),
        root: ROOT.into(),
        shortcuts: Shortcuts {
            desktop: true,
            start_menu: true,
        },
        action,
    }
}

fn exits(replies: &[Reply]) -> Vec<&SetupExit> {
    replies
        .iter()
        .filter_map(|reply| match reply {
            Reply::Event(Event::SetupExited(exit)) => Some(exit),
            _ => None,
        })
        .collect()
}

fn cleanups(host: &FakeHost) -> usize {
    host.count(|call| {
        matches!(
            call,
            Call::Cleanup {
                remove_created: true
            }
        )
    })
}

#[test]
fn the_worker_keeps_an_empty_root_for_retry_and_releases_it_on_close() {
    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());
    let mut host = FakeHost::new();
    host.discoveries.push_back(Ok(Discovery::default()));
    host.registry.push_back(Registry::Absent);
    host.root_empty.push_back(Ok(true));
    host.child_script.push_back(Some(1));
    let replies = serve(
        &mut host,
        &payload,
        vec![Job::Install(request(InstallAction::Run))],
    );
    assert_eq!(
        exits(&replies),
        [&SetupExit::Failed(Failure::EmptyRoot(Some(1)))]
    );
    assert!(matches!(replies.last(), Some(Reply::Closed)));
    assert_eq!(
        cleanups(&host),
        1,
        "the closed channel must release the held empty root"
    );
    let cleanup = host
        .calls
        .iter()
        .rposition(|c| matches!(c, Call::Cleanup { .. }));
    assert_eq!(cleanup, Some(host.calls.len() - 1), "release happens last");
}

#[test]
fn a_recovery_action_without_a_retained_session_fails_closed() {
    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());
    let mut host = FakeHost::new();
    let jobs = vec![
        Job::Install(request(InstallAction::RetryEmpty)),
        Job::Install(request(InstallAction::MarkAgain)),
        Job::Close,
    ];
    let replies = serve(&mut host, &payload, jobs);
    let changed = SetupExit::Failed(Failure::Changed);
    assert_eq!(exits(&replies), [&changed, &changed]);
    assert!(
        host.calls.is_empty(),
        "no host step may run without the held root"
    );
}

#[test]
fn a_scan_error_is_never_clear_and_checks_carry_their_candidate() {
    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());
    let mut host = FakeHost::new();
    host.running
        .push_back(Err(std::io::Error::other("snapshot")));
    let jobs = vec![Job::Rescan(ROOT.into()), Job::Check(candidate(ROOT))];
    let replies = serve(&mut host, &payload, jobs);
    assert!(matches!(
        replies.first(),
        Some(Reply::Event(Event::RunningScanned(
            crate::flow::RunningScan::Failed
        )))
    ));
    assert!(matches!(&replies[1], Reply::Checked { result: Ok(()), .. }));
}

/// A fake host scripted for a fresh install whose Setup.exe exits 0.
fn exit_zero_host() -> FakeHost {
    let mut host = FakeHost::new();
    host.discoveries.push_back(Ok(Discovery::default()));
    host.registry.push_back(Registry::Absent);
    host.registry.push_back(Registry::NamesRoot);
    host.verified.push_back(crate::runner::Verified {
        update_exe: true,
        sq_version: crate::mode::parse_sq_version(&format!(
            "<package><metadata><id>NavoApp</id><version>{VERSION}</version></metadata></package>"
        ))
        .ok(),
    });
    host.child_script.push_back(Some(0));
    host
}

#[test]
fn close_after_an_exit_zero_install_removes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());
    let unmarked = |host: &mut FakeHost| host.marker_writes.push_back(MarkerWrite::Fail);
    let remarked = |host: &mut FakeHost| host.marker_writes.push_back(MarkerWrite::Exists);
    let unlisted = |host: &mut FakeHost| {
        host.registry.clear();
        host.registry.push_back(Registry::Absent);
        host.registry.push_back(Registry::Absent);
    };
    type Script<'a> = &'a dyn Fn(&mut FakeHost);
    let cases: [(Script, Failure); 3] = [
        (&unmarked, Failure::Unmarked),
        (&remarked, Failure::Remarked),
        (&unlisted, Failure::Unlisted),
    ];
    for (script, failure) in cases {
        let mut host = exit_zero_host();
        script(&mut host);
        let jobs = vec![Job::Install(request(InstallAction::Run)), Job::Close];
        let replies = serve(&mut host, &payload, jobs);
        assert_eq!(exits(&replies), [&SetupExit::Failed(failure.clone())]);
        assert_eq!(
            cleanups(&host),
            0,
            "{failure:?}: Close may only drop handles"
        );
        let kept = host.count(|c| {
            matches!(
                c,
                Call::Cleanup {
                    remove_created: false
                }
            )
        });
        assert_eq!(kept, 1, "{failure:?}: Close still closes the session");
    }
}

#[test]
fn an_extract_error_is_a_failed_run_not_a_missing_payload() {
    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());
    let mut host = FakeHost::new();
    host.discoveries.push_back(Ok(Discovery::default()));
    host.extract_ok = false;
    let replies = serve(
        &mut host,
        &payload,
        vec![Job::Install(request(InstallAction::Run))],
    );
    assert_eq!(exits(&replies), [&SetupExit::Failed(Failure::Extract)]);
}

#[test]
fn release_drops_the_held_empty_root_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let payload = payload(dir.path());
    let mut host = FakeHost::new();
    host.discoveries.push_back(Ok(Discovery::default()));
    host.registry.push_back(Registry::Absent);
    host.root_empty.push_back(Ok(true));
    host.child_script.push_back(Some(1));
    let jobs = vec![
        Job::Install(request(InstallAction::Run)),
        Job::Release,
        Job::Rescan(ROOT.into()),
    ];
    serve(&mut host, &payload, jobs);
    assert_eq!(cleanups(&host), 1, "the empty root is removed once");
    let cleanup = host
        .calls
        .iter()
        .position(|c| matches!(c, Call::Cleanup { .. }));
    let rescan = host
        .calls
        .iter()
        .position(|c| matches!(c, Call::Running(_)));
    assert!(cleanup < rescan, "Release must run before the next job");
}
