use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Receiver, channel};
use std::thread::{self, ThreadId};

use super::*;

struct Counting {
    found: Vec<DetectedTool>,
    detects: AtomicUsize,
}

impl ToolPort for Counting {
    fn detect(&self) -> Vec<DetectedTool> {
        self.detects.fetch_add(1, Ordering::SeqCst);
        self.found.clone()
    }

    fn start(
        &self,
        _: Launch,
        _: &LaunchPorts,
        _: Box<dyn FnOnce() + Send>,
    ) -> Result<(), LaunchError> {
        panic!("the menu must not start a tool");
    }
}

struct NoSync;

impl SyncPort for NoSync {
    fn confirm(&self, _: &Path, _: SyncKind) -> Result<Confirmation, SyncFailure> {
        panic!("the menu must not read a confirmation");
    }

    fn run(&self, _: &Path, _: &Confirmation, _: &RunCancel) -> Result<SyncSummary, SyncFailure> {
        panic!("the menu must not run a sync");
    }
}

fn cli_settings() -> GitSettings {
    GitSettings {
        tool: "git-cli".to_owned(),
        ..GitSettings::default()
    }
}

#[test]
fn the_menu_answers_from_memory_and_never_calls_a_port() {
    let port = Arc::new(Counting {
        found: vec![found(ToolKind::GitCli)],
        detects: AtomicUsize::new(0),
    });
    let rig = rig_with(&cli_settings(), Tools::new(port.clone(), Arc::new(NoSync)));
    let repo = Location::new("fixture/repo").unwrap();
    let cold = rig.hooks.menu(&repo);
    assert_eq!(cold.items.len(), 1);
    assert_eq!(port.detects.load(Ordering::SeqCst), 0);
    drop(rig.hooks.tools.detected());
    for _ in 0..3 {
        let warm = rig.hooks.menu(&repo);
        let waiting: Vec<_> = warm
            .items
            .iter()
            .filter(|item| item.pending)
            .map(|item| item.action)
            .collect();
        assert_eq!(waiting, [GitAction::Pull, GitAction::Push]);
        assert!(warm.items.iter().all(|item| item.confirm.is_none()));
        drop(rig.hooks.tools());
        assert!(!rig.hooks.can_diff(&repo));
    }
    assert_eq!(port.detects.load(Ordering::SeqCst), 1);
}

struct Gated {
    inner: Arc<FakeSync>,
    gate: Mutex<Receiver<()>>,
    threads: Mutex<Vec<ThreadId>>,
}

impl SyncPort for Gated {
    fn confirm(&self, repo: &Path, kind: SyncKind) -> Result<Confirmation, SyncFailure> {
        self.gate.lock().unwrap().recv().ok();
        self.threads.lock().unwrap().push(thread::current().id());
        self.inner.confirm(repo, kind)
    }

    fn run(
        &self,
        repo: &Path,
        confirmed: &Confirmation,
        cancel: &RunCancel,
    ) -> Result<SyncSummary, SyncFailure> {
        self.inner.run(repo, confirmed, cancel)
    }
}

fn ask(rig: &Rig, repo: &Location) -> Receiver<Previewed> {
    let (sender, receiver) = channel();
    rig.hooks
        .previews(repo, Box::new(move |replies| drop(sender.send(replies))));
    receiver
}

fn answer(receiver: &Receiver<Previewed>) -> Previewed {
    receiver
        .recv_timeout(Duration::from_secs(30))
        .expect("the previews reply")
}

#[test]
fn requests_for_one_repository_share_one_read_on_one_worker_thread() {
    let (permit, gate) = channel();
    let inner = FakeSync::replying(Ok(pulled()));
    let sync = Arc::new(Gated {
        inner: inner.clone(),
        gate: Mutex::new(gate),
        threads: Mutex::default(),
    });
    let tools = Arc::new(FakeTools {
        found: vec![found(ToolKind::GitCli)],
        ..FakeTools::default()
    });
    let rig = rig_with(&cli_settings(), Tools::new(tools, sync.clone()));
    let repo = Location::new("fixture/repo").unwrap();
    let (first, second) = (ask(&rig, &repo), ask(&rig, &repo));
    for _ in 0..2 {
        permit.send(()).unwrap();
    }
    let (first, second) = (answer(&first), answer(&second));
    assert_eq!(first, second);
    assert_eq!(first.len(), 2);
    assert!(first.iter().all(|(_, text)| text.is_some()));
    assert_eq!(inner.confirms.lock().unwrap().len(), 2);
    for _ in 0..2 {
        permit.send(()).unwrap();
    }
    answer(&ask(&rig, &repo));
    assert_eq!(inner.confirms.lock().unwrap().len(), 4);
    let threads = sync.threads.lock().unwrap();
    assert!(threads.iter().all(|id| *id == threads[0]));
    assert_ne!(threads[0], thread::current().id());
}

#[test]
fn a_repository_that_is_not_waiting_gets_its_own_read() {
    let wired = wired("git-cli", &[ToolKind::GitCli]);
    let other = Location::new("fixture/other").unwrap();
    let (here, there) = (ask(&wired.rig, &wired.repo), ask(&wired.rig, &other));
    assert_eq!(answer(&here).len(), 2);
    assert_eq!(answer(&there).len(), 2);
    assert_eq!(wired.sync.confirms.lock().unwrap().len(), 4);
}

struct PanicsOnce {
    inner: Arc<FakeSync>,
    armed: AtomicBool,
}

impl SyncPort for PanicsOnce {
    fn confirm(&self, repo: &Path, kind: SyncKind) -> Result<Confirmation, SyncFailure> {
        assert!(!self.armed.swap(false, Ordering::SeqCst), "fake port panic");
        self.inner.confirm(repo, kind)
    }

    fn run(
        &self,
        repo: &Path,
        confirmed: &Confirmation,
        cancel: &RunCancel,
    ) -> Result<SyncSummary, SyncFailure> {
        self.inner.run(repo, confirmed, cancel)
    }
}

#[test]
fn a_panicking_read_answers_none_and_later_requests_are_still_served() {
    let sync = Arc::new(PanicsOnce {
        inner: FakeSync::replying(Ok(pulled())),
        armed: AtomicBool::new(true),
    });
    let tools = Arc::new(FakeTools {
        found: vec![found(ToolKind::GitCli)],
        ..FakeTools::default()
    });
    let rig = rig_with(&cli_settings(), Tools::new(tools, sync));
    let repo = Location::new("fixture/repo").unwrap();
    let panicked = answer(&ask(&rig, &repo));
    assert_eq!(panicked, [(GitAction::Pull, None), (GitAction::Push, None)]);
    let later = answer(&ask(&rig, &repo));
    assert!(later.len() == 2 && later.iter().all(|(_, text)| text.is_some()));
}
