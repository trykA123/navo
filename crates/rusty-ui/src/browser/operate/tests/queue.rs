use std::sync::{Condvar, mpsc};

use super::*;
use crate::browser::{DropConfirm, DropRequest, PendingOp, Run};
use rusty_core::{Location, Totals};

struct Gate {
    open: Mutex<bool>,
    changed: Condvar,
}

impl Gate {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            open: Mutex::new(false),
            changed: Condvar::new(),
        })
    }

    fn wait(&self) {
        let (open, timeout) = self
            .changed
            .wait_timeout_while(self.open.lock().unwrap(), Duration::from_secs(10), |open| {
                !*open
            })
            .unwrap();
        assert!(*open && !timeout.timed_out(), "job gate was not released");
    }

    fn release(&self) {
        *self.open.lock().unwrap() = true;
        self.changed.notify_all();
    }
}

fn gated(browser: &mut Browser, fail: bool) -> (Arc<Gate>, mpsc::Receiver<()>) {
    let gate = Gate::new();
    let wait = gate.clone();
    let (entered, started) = mpsc::channel();
    browser.set_operations(Operations {
        log: Arc::new(move |_| {
            entered.send(()).unwrap();
            wait.wait();
            if fail {
                return Err("journal failed".to_owned());
            }
            Ok(Box::new(SharedLog(Arc::default())) as Box<dyn OpLog>)
        }),
        ..Operations::unlogged()
    });
    (gate, started)
}

fn wait_for(browser: &mut Browser, ready: impl Fn(&Browser) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready(browser) {
        assert!(Instant::now() < deadline, "browser condition timed out");
        browser.poll();
        std::thread::yield_now();
    }
}

fn copy(browser: &mut Browser, fs: &FakeFs, source: &str) -> u64 {
    let id = browser
        .drop_into(DropRequest {
            kind: OpKind::Copy,
            sources: vec![fs.location(source)],
            target: fs.location("dest"),
            confirm: DropConfirm::Never,
            source_tab: None,
        })
        .unwrap();
    wait_for(browser, |browser| {
        browser.jobs().iter().any(|job| job.id == id)
    });
    id
}

#[test]
fn slow_volume_runs_jobs_in_order() {
    let fs = fake();
    let mut browser = browser(&fs, home_beside_dest(&fs));
    let ui = std::thread::current().id();
    browser.set_slow_volume(Arc::new(move |_| {
        assert_ne!(std::thread::current().id(), ui);
        true
    }));
    let (gate, started) = gated(&mut browser, false);
    copy(&mut browser, &fs, "home/docs");
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    let second = copy(&mut browser, &fs, "home/a.txt");
    assert_eq!(browser.jobs()[1].status, JobStatus::Queued);
    assert!(!exists(&fs, "dest/a.txt"));
    assert!(!browser.pause_job(second));
    assert!(!browser.dismiss_job(second));
    gate.release();
    settle(&mut browser);
    assert_eq!(browser.jobs()[1].status, JobStatus::Done);
    assert!(exists(&fs, "dest/a.txt"));
    started.recv_timeout(Duration::from_secs(5)).unwrap();
}

#[test]
fn fast_volume_runs_both_jobs_at_once() {
    let fs = fake();
    let mut browser = browser(&fs, home_beside_dest(&fs));
    browser.set_slow_volume(Arc::new(|_| false));
    let (gate, started) = gated(&mut browser, false);
    copy(&mut browser, &fs, "home/docs");
    copy(&mut browser, &fs, "home/a.txt");
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(
        browser
            .jobs()
            .iter()
            .all(|job| job.status == JobStatus::Running)
    );
    gate.release();
    settle(&mut browser);
}

#[test]
fn cancelling_queued_job_never_runs_it_or_touches_files() {
    let fs = fake();
    let mut browser = browser(&fs, home_beside_dest(&fs));
    browser.set_slow_volume(Arc::new(|_| true));
    let (gate, started) = gated(&mut browser, false);
    copy(&mut browser, &fs, "home/docs");
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    let second = copy(&mut browser, &fs, "home/a.txt");
    assert!(browser.cancel_job(second));
    assert_eq!(browser.jobs()[1].status, JobStatus::Cancelled);
    assert_eq!(browser.take_ended(), [second]);
    gate.release();
    settle(&mut browser);
    assert!(started.try_recv().is_err());
    assert!(exists(&fs, "home/a.txt"));
    assert_eq!(fs.calls("home/a.txt", FakeOp::Copy), 0);
    assert!(!exists(&fs, "dest/a.txt"));
}

#[test]
fn queued_job_starts_after_failure() {
    let fs = fake();
    let mut browser = browser(&fs, home_beside_dest(&fs));
    browser.set_slow_volume(Arc::new(|_| true));
    let (gate, started) = gated(&mut browser, true);
    copy(&mut browser, &fs, "home/docs");
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    browser.set_operations(Operations::unlogged());
    copy(&mut browser, &fs, "home/a.txt");
    assert_eq!(browser.jobs()[1].status, JobStatus::Queued);
    gate.release();
    settle(&mut browser);
    assert!(matches!(browser.jobs()[0].status, JobStatus::Failed(_)));
    assert_eq!(browser.jobs()[1].status, JobStatus::Done);
    assert!(exists(&fs, "dest/a.txt"));
}

fn local_job(browser: &mut Browser, source: &str, target: Option<&str>) -> u64 {
    browser.next_op += 1;
    let id = browser.next_op;
    browser.start_job(
        PendingOp {
            id,
            sources: vec![Location::new(source).unwrap()],
            target: target.map(|target| Location::new(target).unwrap()),
            plan: OperationPlan {
                kind: OpKind::Copy,
                steps: Vec::new(),
                conflicts: Vec::new(),
                risks: Vec::new(),
                totals: Totals {
                    files: 0,
                    folders: 0,
                    bytes: 0,
                },
            },
            retain_deletes: None,
            cannot_retain: false,
            source_tab: browser.state.active_tab().id(),
            undo: None,
            slow_volumes: None,
            structure: None,
        },
        Run::Plain,
        None,
    );
    wait_for(browser, |browser| !browser.loading());
    id
}

#[test]
fn two_slow_volumes_wait_for_both_and_preserve_each_volumes_order() {
    let fs = fake();
    let mut browser = browser(&fs, home_beside_dest(&fs));
    browser.set_slow_volume(Arc::new(|_| true));
    let (a, a_started) = gated(&mut browser, false);
    let first = local_job(&mut browser, "A:\\one", None);
    a_started.recv_timeout(Duration::from_secs(5)).unwrap();
    let (b, b_started) = gated(&mut browser, false);
    let second = local_job(&mut browser, "B:\\two", None);
    b_started.recv_timeout(Duration::from_secs(5)).unwrap();
    browser.set_operations(Operations::unlogged());
    let both = local_job(&mut browser, "A:\\three", Some("B:\\dest"));
    let later = local_job(&mut browser, "A:\\four", None);
    a.release();
    wait_for(&mut browser, |browser| {
        browser
            .jobs()
            .iter()
            .any(|job| job.id == first && job.status == JobStatus::Done)
    });
    assert_eq!(
        browser
            .jobs()
            .iter()
            .find(|job| job.id == both)
            .unwrap()
            .status,
        JobStatus::Queued
    );
    assert_eq!(
        browser
            .jobs()
            .iter()
            .find(|job| job.id == later)
            .unwrap()
            .status,
        JobStatus::Queued
    );
    b.release();
    settle(&mut browser);
    assert!(
        browser
            .jobs()
            .iter()
            .filter(|job| [second, both, later].contains(&job.id))
            .all(|job| job.status == JobStatus::Done)
    );
}

#[test]
fn queued_job_starts_after_running_job_is_cancelled() {
    let fs = fake();
    let mut browser = browser(&fs, home_beside_dest(&fs));
    browser.set_slow_volume(Arc::new(|_| true));
    let (gate, started) = gated(&mut browser, false);
    let first = copy(&mut browser, &fs, "home/docs");
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    browser.set_operations(Operations::unlogged());
    copy(&mut browser, &fs, "home/a.txt");
    assert!(browser.cancel_job(first));
    gate.release();
    settle(&mut browser);
    assert_eq!(browser.jobs()[0].status, JobStatus::Cancelled);
    assert_eq!(browser.jobs()[1].status, JobStatus::Done);
    assert!(exists(&fs, "dest/a.txt"));
}

fn undo_offer(fs: &FakeFs, restore_pending: bool) -> crate::browser::UndoOffer {
    fs.add_file("home/renamed.txt", 3);
    crate::browser::UndoOffer {
        job: 99,
        plan: OperationPlan {
            kind: OpKind::Rename,
            steps: vec![rusty_core::Step::Rename {
                source: fs.location("home/original.txt"),
                target: fs.location("home/renamed.txt"),
            }],
            totals: Totals {
                files: 1,
                folders: 0,
                bytes: 0,
            },
            conflicts: Vec::new(),
            risks: Vec::new(),
        },
        done: vec![0],
        created_at: 0,
        restore_pending,
        kind: crate::browser::UndoKind::Rename,
    }
}

struct UndoLog(rusty_fs::FileIdentity);

impl JobLog for UndoLog {
    fn step(&mut self, _: u32, _: StepOutcome) -> Result<(), rusty_core::FsError> {
        Ok(())
    }
    fn undone(&mut self, _: u32, _: StepOutcome) -> Result<(), rusty_core::FsError> {
        Ok(())
    }
    fn rename_identity(
        &self,
        _: u32,
    ) -> Result<Option<rusty_fs::FileIdentity>, rusty_core::FsError> {
        Ok(Some(self.0))
    }
    fn journal_held(&mut self, _: &rusty_fs::HeldFile) -> Result<bool, rusty_core::FsError> {
        Ok(false)
    }
}

impl OpLog for UndoLog {
    fn finish(&mut self, _: &JobOutcome, _: &dyn ProviderOps) -> Result<(), String> {
        Ok(())
    }
}

fn undo_hooks(browser: &mut Browser, fs: &FakeFs) -> mpsc::Receiver<()> {
    let identity = fs
        .file_identity(&fs.location("home/renamed.txt"))
        .unwrap()
        .unwrap();
    let (entered, started) = mpsc::channel();
    browser.set_rename_hooks(Some(crate::browser::RenameHooks {
        last: Arc::new(|| Ok(None)),
        undo_log: Arc::new(move |_| {
            entered.send(()).unwrap();
            Ok(Box::new(UndoLog(identity)) as Box<dyn OpLog>)
        }),
        retire: Arc::new(|_| Ok(())),
        presets: Arc::new(|| Ok(crate::browser::RenamePresetList::default())),
        save: Arc::new(|_, _| Ok(())),
        delete: Arc::new(|_| Ok(())),
        log: None,
    }));
    started
}

#[test]
fn undo_and_restore_wait_for_the_running_job() {
    for restore in [false, true] {
        let fs = fake();
        let mut browser = browser(&fs, home_beside_dest(&fs));
        browser.set_slow_volume(Arc::new(|_| true));
        let (gate, started) = gated(&mut browser, false);
        copy(&mut browser, &fs, "home/docs");
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        let offer = undo_offer(&fs, restore);
        let undo_started = undo_hooks(&mut browser, &fs);
        assert!(browser.undo_rename(offer));
        wait_for(&mut browser, |browser| !browser.loading());
        assert_eq!(browser.jobs()[1].status, JobStatus::Queued);
        assert!(undo_started.try_recv().is_err());
        assert!(exists(&fs, "home/renamed.txt"));
        gate.release();
        settle(&mut browser);
        undo_started.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(browser.jobs()[1].status, JobStatus::Done);
        assert!(exists(&fs, "home/original.txt"));
        assert!(!exists(&fs, "home/renamed.txt"));
    }
}

#[test]
fn cancelling_queued_undo_never_opens_its_log_or_changes_files() {
    let fs = fake();
    let mut browser = browser(&fs, home_beside_dest(&fs));
    browser.set_slow_volume(Arc::new(|_| true));
    let (gate, started) = gated(&mut browser, false);
    copy(&mut browser, &fs, "home/docs");
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    let offer = undo_offer(&fs, true);
    let undo_started = undo_hooks(&mut browser, &fs);
    assert!(browser.undo_rename(offer));
    let id = browser.jobs()[1].id;
    assert!(browser.cancel_job(id));
    gate.release();
    settle(&mut browser);
    assert!(undo_started.try_recv().is_err());
    assert!(exists(&fs, "home/renamed.txt"));
    assert!(!exists(&fs, "home/original.txt"));
    assert_eq!(fs.calls("home/renamed.txt", FakeOp::Rename), 0);
}

fn queued_info() -> crate::job_card::JobInfo {
    crate::job_card::JobInfo {
        id: 1,
        kind: OpKind::Copy,
        items: 1,
        from: None,
        target: None,
        current: None,
        bytes_done: 0,
        bytes_total: 7,
        files_done: 0,
        files_total: 1,
        state: crate::job_row::JobRowState::Queued,
        speed: None,
        pause: crate::job_card::Pause::Off,
        rename: crate::job_card::RenameNote::default(),
    }
}

#[test]
fn queued_jobs_stay_active_in_summary_and_activity_until_completion() {
    use crate::job_card::JobsSummary;
    use crate::job_row::JobRowState;

    let mut info = queued_info();
    let summary = JobsSummary::of(std::slice::from_ref(&info));
    assert!(summary.is_running);
    assert!(!summary.is_ended());
    let mut activity = crate::activity::Activity::default();
    let now = Instant::now();
    activity.sync(std::slice::from_ref(&info), now);
    let item = activity.primary().unwrap();
    assert!(item.can_cancel);
    assert!(!item.can_pause);
    assert_eq!(item.progress, None);
    info.state = JobRowState::Running;
    activity.sync(std::slice::from_ref(&info), now);
    assert!(activity.primary().unwrap().can_pause);
    info.state = JobRowState::Done;
    activity.sync(&[info], now);
    assert_eq!(
        activity.primary().unwrap().kind,
        crate::activity::ActivityKind::Done
    );
}

struct QueuedCard;

impl gpui_kit::Render for QueuedCard {
    fn render(
        &mut self,
        _: &mut gpui_kit::Window,
        _: &mut gpui_kit::Context<Self>,
    ) -> impl gpui_kit::IntoElement {
        crate::job_card::JobCard::new(vec![queued_info()])
            .on_pause(|_, _, _| {})
            .on_resume(|_, _, _| {})
            .on_cancel(|_, _, _| {})
            .on_dismiss(|_, _, _| {})
    }
}

#[gpui_kit::test]
fn queued_card_only_offers_cancel_in_both_themes(cx: &mut gpui_kit::TestAppContext) {
    use crate::tokens::{self, Mode};
    use gpui_kit::AppContext as _;
    use gpui_kit::test::TestWindowExt;

    let (window, _) = crate::test_window::open(cx, |_, _| QueuedCard);
    for mode in [Mode::Light, Mode::Dark] {
        cx.update(|cx| tokens::apply(mode, cx));
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find(("job-cancel", 1_u64)).visible());
            assert!(window.try_find(("job-pause", 1_u64)).is_none());
            assert!(window.try_find(("job-resume", 1_u64)).is_none());
            assert!(window.try_find(("job-dismiss", 1_u64)).is_none());
        })
        .unwrap();
    }
}
