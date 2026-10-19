use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rusty_core::{AppState, Command, Direction, FsErrorKind, OpKind, OperationPlan, apply};
use rusty_fs::{FakeFs, FakeOp, Fault, JobLog, JobOutcome, Provider, ProviderOps, StepOutcome};

use super::{JobStatus, OpLog, Operations};
use crate::browser::Browser;

mod mode;
mod pause;

fn fake() -> Arc<FakeFs> {
    let fs = FakeFs::new();
    fs.add_folder("home/docs");
    fs.add_file("home/docs/report.txt", 7);
    fs.add_file("home/a.txt", 3);
    fs.add_folder("dest");
    Arc::new(fs)
}

fn home_beside_dest(fs: &FakeFs) -> AppState {
    let mut state = AppState::new(fs.location("home"));
    let _ = apply(
        &mut state,
        &Command::LayoutSplit {
            direction: Direction::Right,
        },
    );
    let location = fs.location("dest");
    let _ = apply(&mut state, &Command::PaneNavigate { location });
    let dest = state.focused_pane();
    for direction in Direction::ALL {
        if state.focused_pane() != dest {
            break;
        }
        let _ = apply(&mut state, &Command::LayoutFocus { direction });
    }
    state
}

fn browser(fs: &Arc<FakeFs>, state: AppState) -> Browser {
    let provider: Arc<dyn ProviderOps> = fs.clone();
    let mut browser = Browser::new(state, provider, Arc::new(|| {}));
    browser.start();
    settle(&mut browser);
    browser
}

fn settle(browser: &mut Browser) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while (browser.loading() || browser.running()) && Instant::now() < deadline {
        browser.poll();
        std::thread::sleep(Duration::from_millis(1));
    }
    browser.poll();
}

fn exists(fs: &FakeFs, path: &str) -> bool {
    fs.metadata(&fs.location(path)).is_ok()
}

fn names_in(browser: &Browser, path: &str) -> Vec<String> {
    let state = browser.state();
    let window = state.window(state.focused_window());
    window
        .into_iter()
        .flat_map(|w| w.panes())
        .map(|pane| pane.active())
        .find(|tab| tab.location().as_str().ends_with(path))
        .and_then(|tab| tab.listing())
        .map(|l| l.entries().iter().map(|e| e.name.clone()).collect())
        .unwrap_or_default()
}

#[derive(Default)]
struct Record {
    steps: Vec<(u32, StepOutcome)>,
    finished: Option<JobOutcome>,
}

struct SharedLog(Arc<Mutex<Record>>);

impl JobLog for SharedLog {
    fn step(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), rusty_core::FsError> {
        if let Ok(mut record) = self.0.lock() {
            record.steps.push((seq, outcome));
        }
        Ok(())
    }
}

impl OpLog for SharedLog {
    fn finish(&mut self, outcome: &JobOutcome) {
        if let Ok(mut record) = self.0.lock() {
            record.finished = Some(outcome.clone());
        }
    }
}

#[test]
fn copy_needs_a_second_pane() {
    let fs = fake();
    let mut browser = browser(&fs, AppState::new(fs.location("home")));
    assert!(!browser.prepare(OpKind::Copy));
    assert!(browser.prepare(OpKind::Recycle));
}

#[test]
fn prepare_plans_the_cursor_row_into_the_other_pane() {
    let fs = fake();
    let mut browser = browser(&fs, home_beside_dest(&fs));
    assert!(browser.prepare(OpKind::Copy));
    settle(&mut browser);
    let pending = browser.pending().expect("a plan");
    assert_eq!(pending.plan.kind, OpKind::Copy);
    assert_eq!(pending.sources, [fs.location("home/docs")]);
    assert_eq!(pending.target, Some(fs.location("dest")));
    assert_eq!(pending.plan.totals.files, 1);
}

#[test]
fn marked_rows_win_over_the_cursor() {
    let fs = fake();
    let mut browser = browser(&fs, home_beside_dest(&fs));
    browser.command(&Command::SelectionToggleDown).unwrap();
    browser.command(&Command::SelectionToggleDown).unwrap();
    browser.prepare(OpKind::Copy);
    settle(&mut browser);
    let sources = browser.pending().map(|p| p.sources.clone());
    assert_eq!(
        sources,
        Some(vec![fs.location("home/docs"), fs.location("home/a.txt")])
    );
}

#[test]
fn confirm_copies_reloads_both_panes_and_clears_the_marks() {
    let fs = fake();
    let mut browser = browser(&fs, home_beside_dest(&fs));
    browser.command(&Command::SelectionToggle).unwrap();
    browser.prepare(OpKind::Copy);
    settle(&mut browser);
    assert!(browser.confirm());
    settle(&mut browser);
    assert!(exists(&fs, "dest/docs/report.txt"));
    assert!(exists(&fs, "home/docs/report.txt"));
    assert_eq!(browser.jobs()[0].status, JobStatus::Done);
    assert_eq!(browser.jobs()[0].progress.files_done, 1);
    assert_eq!(names_in(&browser, "dest"), ["docs"]);
    assert!(browser.state().active_tab().selection().is_empty());
    assert!(browser.pending().is_none());
}

#[test]
fn move_leaves_nothing_behind() {
    let fs = fake();
    let mut browser = browser(&fs, home_beside_dest(&fs));
    browser.prepare(OpKind::Move);
    settle(&mut browser);
    browser.confirm();
    settle(&mut browser);
    assert!(exists(&fs, "dest/docs/report.txt"));
    assert!(!exists(&fs, "home/docs"));
    assert_eq!(names_in(&browser, "home"), ["a.txt"]);
}

#[test]
fn recycle_sends_the_row_to_the_bin() {
    let fs = fake();
    let mut browser = browser(&fs, home_beside_dest(&fs));
    browser.prepare(OpKind::Recycle);
    settle(&mut browser);
    assert!(browser.pending().is_some_and(|p| p.target.is_none()));
    browser.confirm();
    settle(&mut browser);
    assert_eq!(fs.recycled().len(), 1);
}

#[test]
fn dismiss_drops_the_plan_and_touches_nothing() {
    let fs = fake();
    let mut browser = browser(&fs, home_beside_dest(&fs));
    browser.prepare(OpKind::Delete);
    settle(&mut browser);
    assert!(browser.dismiss());
    assert!(!browser.dismiss());
    assert!(!browser.confirm());
    assert!(exists(&fs, "home/docs/report.txt"));
    assert!(browser.jobs().is_empty());
}

#[test]
fn a_folder_cannot_be_copied_into_itself() {
    let fs = fake();
    let mut state = AppState::new(fs.location("home"));
    let _ = apply(
        &mut state,
        &Command::LayoutSplit {
            direction: Direction::Right,
        },
    );
    let mut browser = browser(&fs, state);
    assert!(!browser.prepare(OpKind::Copy));
    let docs = fs.location("home/docs");
    browser
        .command(&Command::PaneNavigate { location: docs })
        .unwrap();
    for direction in Direction::ALL {
        let _ = browser.command(&Command::LayoutFocus { direction });
        settle(&mut browser);
        if browser.state().active_tab().location() == &fs.location("home") {
            break;
        }
    }
    assert!(!browser.prepare(OpKind::Move));
}

#[test]
fn a_scan_failure_shows_a_failed_job() {
    let fs = fake();
    let mut browser = browser(&fs, home_beside_dest(&fs));
    fs.fault(
        "home/docs",
        FakeOp::Metadata,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    browser.prepare(OpKind::Copy);
    settle(&mut browser);
    assert!(browser.pending().is_none());
    assert!(matches!(browser.jobs()[0].status, JobStatus::Failed(_)));
}

#[test]
fn a_journal_that_cannot_start_stops_the_job() {
    let fs = fake();
    let mut browser = browser(&fs, home_beside_dest(&fs));
    browser.set_operations(Operations {
        log: Arc::new(|_: &OperationPlan| Err("journal is read-only".to_owned())),
        protected: Vec::new(),
    });
    browser.prepare(OpKind::Copy);
    settle(&mut browser);
    browser.confirm();
    settle(&mut browser);
    assert_eq!(
        browser.jobs()[0].status,
        JobStatus::Failed("journal is read-only".to_owned())
    );
    assert!(!exists(&fs, "dest/docs"));
}

#[test]
fn every_step_and_the_end_reach_the_log() {
    let fs = fake();
    let record = Arc::new(Mutex::new(Record::default()));
    let shared = Arc::clone(&record);
    let mut browser = browser(&fs, home_beside_dest(&fs));
    browser.set_operations(Operations {
        log: Arc::new(move |_: &OperationPlan| {
            Ok(Box::new(SharedLog(Arc::clone(&shared))) as Box<dyn OpLog>)
        }),
        protected: Vec::new(),
    });
    browser.prepare(OpKind::Copy);
    settle(&mut browser);
    let steps = browser.pending().map_or(0, |p| p.plan.steps.len());
    browser.confirm();
    settle(&mut browser);
    let record = record.lock().unwrap();
    assert_eq!(record.steps.len(), steps);
    assert_eq!(record.finished, Some(JobOutcome::Done));
}

#[test]
fn a_cancelled_job_stops_before_its_first_step() {
    let fs = fake();
    let gate = Arc::new(AtomicBool::new(false));
    let open = Arc::clone(&gate);
    let mut browser = browser(&fs, home_beside_dest(&fs));
    browser.set_operations(Operations {
        log: Arc::new(move |_: &OperationPlan| {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !open.load(Ordering::Acquire) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(1));
            }
            Ok(Box::new(SharedLog(Arc::default())) as Box<dyn OpLog>)
        }),
        protected: Vec::new(),
    });
    browser.prepare(OpKind::Copy);
    settle(&mut browser);
    browser.confirm();
    let id = browser.jobs()[0].id;
    assert!(browser.cancel_job(id));
    assert!(!browser.cancel_job(id + 1));
    gate.store(true, Ordering::Release);
    settle(&mut browser);
    assert_eq!(browser.jobs()[0].status, JobStatus::Cancelled);
    assert!(!exists(&fs, "dest/docs"));
}

#[test]
fn a_protected_source_is_flagged_in_the_plan() {
    let fs = fake();
    let mut browser = browser(&fs, home_beside_dest(&fs));
    browser.set_operations(Operations {
        protected: vec![fs.location("home/docs")],
        ..Operations::unlogged()
    });
    browser.prepare(OpKind::Delete);
    settle(&mut browser);
    let risks = browser.pending().map(|p| p.plan.risks.clone()).unwrap();
    assert!(
        risks
            .iter()
            .any(|risk| matches!(risk, rusty_core::Risk::Protected(_)))
    );
}

#[test]
fn shutdown_cancels_a_running_job_and_waits_for_its_end() {
    let fs = fake();
    let gate = Arc::new(AtomicBool::new(false));
    let open = Arc::clone(&gate);
    let record = Arc::new(Mutex::new(Record::default()));
    let shared = Arc::clone(&record);
    let mut browser = browser(&fs, home_beside_dest(&fs));
    browser.set_operations(Operations {
        log: Arc::new(move |_: &OperationPlan| {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !open.load(Ordering::Acquire) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(1));
            }
            Ok(Box::new(SharedLog(Arc::clone(&shared))) as Box<dyn OpLog>)
        }),
        protected: Vec::new(),
    });
    browser.prepare(OpKind::Copy);
    settle(&mut browser);
    browser.confirm();
    let opener = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(30));
        gate.store(true, Ordering::Release);
    });
    assert!(browser.shutdown(Duration::from_secs(5)));
    let _ = opener.join();
    assert_eq!(browser.jobs()[0].status, JobStatus::Cancelled);
    assert_eq!(record.lock().unwrap().finished, Some(JobOutcome::Cancelled));
    assert!(!exists(&fs, "dest/docs"));
}

#[test]
fn shutdown_with_no_jobs_returns_at_once() {
    let fs = fake();
    let mut browser = browser(&fs, home_beside_dest(&fs));
    assert!(browser.shutdown(Duration::ZERO));
}

#[test]
fn a_job_that_panics_ends_as_failed() {
    let fs = fake();
    let mut browser = browser(&fs, home_beside_dest(&fs));
    browser.set_operations(Operations {
        log: Arc::new(|_: &OperationPlan| -> Result<Box<dyn OpLog>, String> {
            panic!("journal broke")
        }),
        protected: Vec::new(),
    });
    browser.prepare(OpKind::Copy);
    settle(&mut browser);
    browser.confirm();
    settle(&mut browser);
    assert!(matches!(browser.jobs()[0].status, JobStatus::Failed(_)));
}

#[test]
fn a_finished_job_is_reported_once_and_can_be_forgotten() {
    let fs = fake();
    let mut browser = browser(&fs, home_beside_dest(&fs));
    browser.prepare(OpKind::Copy);
    settle(&mut browser);
    browser.confirm();
    settle(&mut browser);
    let id = browser.jobs()[0].id;
    assert_eq!(browser.take_ended(), [id]);
    assert!(browser.take_ended().is_empty());
    assert!(browser.forget_jobs(&[id]));
    assert!(browser.jobs().is_empty());
}

#[test]
fn a_failed_job_stays_until_dismissed() {
    let fs = fake();
    let mut browser = browser(&fs, home_beside_dest(&fs));
    browser.set_operations(Operations {
        log: Arc::new(|_: &OperationPlan| Err("journal is read-only".to_owned())),
        protected: Vec::new(),
    });
    browser.prepare(OpKind::Copy);
    settle(&mut browser);
    browser.confirm();
    settle(&mut browser);
    let id = browser.jobs()[0].id;
    assert!(browser.take_ended().is_empty());
    assert!(!browser.forget_jobs(&[id + 1]));
    assert!(browser.dismiss_job(id));
    assert!(browser.jobs().is_empty());
}

#[test]
fn a_running_job_is_never_forgotten() {
    let fs = fake();
    let gate = Arc::new(AtomicBool::new(false));
    let open = Arc::clone(&gate);
    let mut browser = browser(&fs, home_beside_dest(&fs));
    browser.set_operations(Operations {
        log: Arc::new(move |_: &OperationPlan| {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !open.load(Ordering::Acquire) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(1));
            }
            Ok(Box::new(SharedLog(Arc::default())) as Box<dyn OpLog>)
        }),
        protected: Vec::new(),
    });
    browser.prepare(OpKind::Copy);
    settle(&mut browser);
    browser.confirm();
    let id = browser.jobs()[0].id;
    assert!(!browser.forget_jobs(&[id]));
    assert!(!browser.dismiss_job(id));
    gate.store(true, Ordering::Release);
    settle(&mut browser);
}

#[test]
fn a_new_job_keeps_failed_jobs_until_they_are_dismissed() {
    let fs = fake();
    let mut browser = browser(&fs, home_beside_dest(&fs));
    fs.fault(
        "home/docs",
        FakeOp::Metadata,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    browser.prepare(OpKind::Copy);
    settle(&mut browser);
    browser.prepare(OpKind::Copy);
    settle(&mut browser);
    browser.confirm();
    settle(&mut browser);
    let statuses: Vec<bool> = browser
        .jobs()
        .iter()
        .map(|job| matches!(job.status, JobStatus::Failed(_)))
        .collect();
    assert_eq!(statuses, [true, false]);
    assert_eq!(browser.jobs()[1].status, JobStatus::Done);
}
