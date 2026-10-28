use std::sync::Arc;
use std::time::{Duration, Instant};

use rusty_core::retention::{RetentionEntry, RetentionPolicy};
use rusty_core::{ConflictPolicy, Location, OpKind, OperationPlan, PlanRequest, plan};
use rusty_fs::{CancelToken, FakeFs, JobOutcome, Provider, ProviderOps, scan};
use rusty_store::{RetentionCatalog, StoreDirs};
use rusty_ui::browser::{LoggedOperation, OpLog};

use super::super::super::mutation_lock::{Mode, Wait, lease};
use super::{MaintenanceTimer, Pass, Schedule, cycle, pass};

const DAY_MS: i64 = 24 * 60 * 60 * 1000;
const START_MS: i64 = 1_000;

struct Profile {
    _root: tempfile::TempDir,
    dirs: StoreDirs,
    journal: crate::jobs::SharedJournal,
    fs: Arc<FakeFs>,
}

impl Profile {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let dirs = StoreDirs::in_profile(root.path());
        dirs.ensure().unwrap();
        std::fs::write(dirs.settings_file(), "[retention]\nretain-deletes = true\n").unwrap();
        let journal = crate::jobs::open(&dirs.journal_file()).unwrap();
        let fs = Arc::new(FakeFs::new());
        fs.add_folder("home");
        fs.set_available_space("home", Some(1 << 40));
        Self {
            _root: root,
            dirs,
            journal,
            fs,
        }
    }

    fn at(&self, path: &str) -> Location {
        self.fs.location(path)
    }

    fn delete(&self, path: &str) {
        self.fs.add_file(path, 8);
        let ops = self.fs.as_ref() as &dyn ProviderOps;
        let plan = delete_plan(ops, &[self.at(path)]);
        let factory = crate::jobs::experimental_factory(self.journal.clone(), self.dirs.clone());
        let mut log = factory(&plan).unwrap();
        assert_eq!(run(log.as_mut(), &plan, ops), Some(JobOutcome::Done));
        log.finish(&JobOutcome::Done, ops).unwrap();
    }

    fn entries(&self) -> Vec<RetentionEntry> {
        let (db, _) = crate::crash::open_db(&self.dirs.journal_file()).unwrap();
        RetentionCatalog::new(db)
            .unwrap()
            .live_after(0, 1000)
            .unwrap()
    }

    fn live(&self) -> usize {
        self.entries().len()
    }

    fn pass(&self, now: i64) -> Pass {
        pass(&self.dirs, self.fs.as_ref(), (now, &CancelToken::new()))
    }
}

fn delete_plan(ops: &dyn ProviderOps, roots: &[Location]) -> OperationPlan {
    plan(&PlanRequest {
        kind: OpKind::Delete,
        items: scan(ops, roots, &CancelToken::new()).unwrap(),
        target: None,
        existing: vec![],
        policy: ConflictPolicy::Ask,
        same_volume: true,
        protected: vec![],
    })
}

fn run(log: &mut dyn OpLog, plan: &OperationPlan, ops: &dyn ProviderOps) -> Option<JobOutcome> {
    let cancel = CancelToken::new();
    log.check(LoggedOperation {
        plan,
        ops,
        cancel: &cancel,
        progress: &mut |_| {},
        waiting: &mut |_| {},
        retain_deletes: Some(RetentionPolicy::default()),
    })
    .unwrap();
    log.execute(LoggedOperation {
        plan,
        ops,
        cancel: &cancel,
        progress: &mut |_| {},
        waiting: &mut |_| {},
        retain_deletes: Some(RetentionPolicy::default()),
    })
}

fn schedule(now: i64) -> Schedule {
    Schedule {
        now: Arc::new(move || now),
        ..Schedule::default()
    }
}

#[test]
fn maintenance_timer_startup_pass_runs_at_once_and_then_every_six_hours() {
    let profile = Profile::new();
    let (mut delays, mut passes) = (Vec::new(), Vec::new());
    cycle(
        &schedule(START_MS),
        |delay| {
            delays.push(delay);
            delays.len() < 3
        },
        |now| {
            passes.push(now);
            profile.pass(now)
        },
    );
    assert_eq!(
        delays,
        [
            Duration::ZERO,
            Duration::from_secs(6 * 3600),
            Duration::from_secs(6 * 3600)
        ]
    );
    assert_eq!(passes, [START_MS, START_MS]);
    assert_eq!(
        profile.pass(START_MS),
        Pass::Ran {
            volumes: 0,
            purged: 0,
            bytes: 0,
            pruned: 0
        }
    );
}

#[test]
fn maintenance_timer_held_lease_defers_the_pass_and_retries_after_ten_minutes() {
    let profile = Profile::new();
    let mut held = Some(lease(&profile.dirs.data, Mode::Exclusive, Wait::bounded()).unwrap());
    let (mut delays, mut outcomes) = (Vec::new(), Vec::new());
    cycle(
        &schedule(START_MS),
        |delay| {
            delays.push(delay);
            if delays.len() == 2 {
                drop(held.take());
            }
            delays.len() < 3
        },
        |now| {
            let outcome = profile.pass(now);
            outcomes.push(outcome.clone());
            outcome
        },
    );
    assert_eq!(
        delays,
        [
            Duration::ZERO,
            Duration::from_secs(600),
            Duration::from_secs(6 * 3600)
        ]
    );
    assert_eq!(outcomes[0], Pass::Busy);
    assert!(matches!(outcomes[1], Pass::Ran { .. }));
}

#[test]
fn maintenance_timer_pass_purges_an_expired_entry_but_not_a_fresh_one() {
    let profile = Profile::new();
    profile.delete("home/old.txt");
    let container = profile.entries()[0].record.container.clone();
    assert_eq!(
        profile.pass(super::super::super::now_ms()),
        Pass::Ran {
            volumes: 1,
            purged: 0,
            bytes: 0,
            pruned: 0
        }
    );
    assert_eq!(profile.live(), 1);
    let later = super::super::super::now_ms() + 4 * DAY_MS;
    assert_eq!(
        profile.pass(later),
        Pass::Ran {
            volumes: 1,
            purged: 1,
            bytes: 8,
            pruned: 0
        }
    );
    assert_eq!(profile.live(), 0);
    assert!(profile.fs.metadata(&container).is_err());
}

#[test]
fn maintenance_timer_pass_prunes_a_purged_row_after_thirty_days() {
    let profile = Profile::new();
    profile.delete("home/old.txt");
    let now = super::super::super::now_ms();
    assert!(matches!(
        profile.pass(now + 4 * DAY_MS),
        Pass::Ran {
            purged: 1,
            pruned: 0,
            ..
        }
    ));
    let rows = || {
        let (db, _) = crate::crash::open_db(&profile.dirs.journal_file()).unwrap();
        RetentionCatalog::new(db)
            .unwrap()
            .list_after(0, 1000)
            .unwrap()
            .len()
    };
    assert_eq!(rows(), 1);
    assert!(matches!(
        profile.pass(now + 29 * DAY_MS),
        Pass::Ran { pruned: 0, .. }
    ));
    assert_eq!(rows(), 1);
    assert!(matches!(
        profile.pass(now + 31 * DAY_MS),
        Pass::Ran { pruned: 1, .. }
    ));
    assert_eq!(rows(), 0);
}

#[test]
fn maintenance_timer_pass_expires_old_entries_when_both_flags_are_off() {
    let profile = Profile::new();
    profile.delete("home/old.txt");
    std::fs::write(
        profile.dirs.settings_file(),
        "[retention]\nretain-deletes = false\nretain-overwrites = false\n",
    )
    .unwrap();
    let later = super::super::super::now_ms() + 4 * DAY_MS;
    let fresh = Profile::new();
    fresh.delete("home/new.txt");
    std::fs::copy(profile.dirs.settings_file(), fresh.dirs.settings_file()).unwrap();
    assert_eq!(
        fresh.pass(super::super::super::now_ms()),
        Pass::Ran {
            volumes: 1,
            purged: 0,
            bytes: 0,
            pruned: 0
        }
    );
    assert_eq!(fresh.live(), 1);
    assert_eq!(
        profile.pass(later),
        Pass::Ran {
            volumes: 1,
            purged: 1,
            bytes: 8,
            pruned: 0
        }
    );
    assert_eq!(profile.live(), 0);
}

#[test]
fn maintenance_timer_failed_pass_retries_after_ten_minutes() {
    let mut delays = Vec::new();
    cycle(
        &schedule(START_MS),
        |delay| {
            delays.push(delay);
            delays.len() < 2
        },
        |_| Pass::Failed,
    );
    assert_eq!(delays, [Duration::ZERO, Duration::from_secs(600)]);
}

#[test]
fn maintenance_timer_does_not_start_with_no_retention_or_without_a_profile() {
    let profile = Profile::new();
    let ops = profile.fs.clone() as Arc<dyn ProviderOps>;
    assert!(!MaintenanceTimer::start(Some(&profile.dirs), false, ops.clone()).is_running());
    assert!(!MaintenanceTimer::start(None, true, ops).is_running());
}

#[test]
fn maintenance_timer_thread_runs_the_startup_pass_and_stops_at_once() {
    let profile = Profile::new();
    profile.delete("home/old.txt");
    let later = super::super::super::now_ms() + 4 * DAY_MS;
    let timer = MaintenanceTimer::with(Some(&profile.dirs), profile.fs.clone(), schedule(later));
    assert!(timer.is_running());
    let deadline = Instant::now() + Duration::from_secs(10);
    while profile.live() > 0 && Instant::now() < deadline {
        std::thread::park_timeout(Duration::from_millis(20));
    }
    assert_eq!(profile.live(), 0);
    let stopping = Instant::now();
    timer.stop();
    assert!(stopping.elapsed() < Duration::from_millis(500));
    assert!(!timer.is_running());
    timer.stop();
}
