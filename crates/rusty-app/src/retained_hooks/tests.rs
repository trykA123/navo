use std::sync::Arc;
use std::time::{Duration, Instant};

use rusty_core::retention::RetentionPolicy;
use rusty_core::{ConflictPolicy, Location, OpKind, OperationPlan, PlanRequest, plan};
use rusty_fs::{CancelToken, FakeFs, JobOutcome, Provider, ProviderOps, scan};
use rusty_store::StoreDirs;
use rusty_ui::browser::{
    LoggedOperation, OpLog, RetainedHooks, RetainedKind, RetainedRow, RetainedState,
};

use super::hooks;
use crate::jobs::{SharedJournal, experimental_factory};

struct Profile {
    _root: tempfile::TempDir,
    dirs: StoreDirs,
    journal: SharedJournal,
    fs: Arc<FakeFs>,
}

impl Profile {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let dirs = StoreDirs::in_profile(root.path());
        dirs.ensure().unwrap();
        std::fs::write(dirs.settings_file(), "[retention]\nretain-deletes = true\n").unwrap();
        let journal = crate::jobs::open(&dirs.journal_file()).unwrap();
        Self {
            _root: root,
            dirs,
            journal,
            fs: Arc::new(FakeFs::new()),
        }
    }

    fn hooks(&self) -> Arc<dyn RetainedHooks> {
        hooks(Some(&self.dirs), true, self.fs.clone())
    }

    fn at(&self, path: &str) -> Location {
        self.fs.location(path)
    }

    fn start_delete(&self, paths: &[&str]) -> Box<dyn OpLog> {
        let roots: Vec<_> = paths.iter().map(|path| self.at(path)).collect();
        let ops = self.fs.as_ref() as &dyn ProviderOps;
        let plan = delete_plan(ops, &roots);
        let mut log = experimental_factory(self.journal.clone(), self.dirs.clone())(&plan).unwrap();
        assert_eq!(run(log.as_mut(), &plan, ops), Some(JobOutcome::Done));
        log
    }

    fn delete(&self, paths: &[&str]) {
        let mut log = self.start_delete(paths);
        log.finish(&JobOutcome::Done, self.fs.as_ref()).unwrap();
    }

    fn exists(&self, path: &str) -> bool {
        self.fs.metadata(&self.at(path)).is_ok()
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

fn originals(rows: &[RetainedRow]) -> Vec<String> {
    rows.iter()
        .map(|row| row.original.as_str().to_owned())
        .collect()
}

#[test]
fn list_pages_live_entries_in_ascending_id_order() {
    let profile = Profile::new();
    for name in ["home/a", "home/b", "home/c"] {
        profile.fs.add_file(name, 8);
        profile.delete(&[name]);
    }
    let hooks = profile.hooks();
    let first = hooks.list(None, 2).unwrap();
    assert_eq!(
        originals(&first),
        [profile.at("home/a"), profile.at("home/b")].map(|at| at.as_str().to_owned())
    );
    assert!(first[0].id < first[1].id);
    assert!(first.iter().all(|row| {
        row.kind == RetainedKind::Delete && row.state == RetainedState::Ready && row.bytes == 8
    }));
    let rest = hooks.list(Some(first[1].id), 2).unwrap();
    assert_eq!(originals(&rest), [profile.at("home/c").as_str().to_owned()]);
    assert!(hooks.list(Some(rest[0].id), 2).unwrap().is_empty());
    assert!(hooks.list(None, 0).unwrap().is_empty());
}

#[test]
fn restore_to_a_free_original_path_returns_it_and_leaves_the_list() {
    let profile = Profile::new();
    profile.fs.add_file("home/a.txt", 8);
    profile.delete(&["home/a.txt"]);
    assert!(!profile.exists("home/a.txt"));
    let hooks = profile.hooks();
    let id = hooks.list(None, 10).unwrap()[0].id;
    assert_eq!(hooks.restore(id).unwrap(), profile.at("home/a.txt"));
    assert!(profile.exists("home/a.txt"));
    assert!(hooks.list(None, 10).unwrap().is_empty());
}

#[test]
fn restore_over_a_new_file_keeps_both_with_a_restored_name() {
    let profile = Profile::new();
    profile.fs.add_file("home/a.txt", 8);
    profile.delete(&["home/a.txt"]);
    profile.fs.add_file("home/a.txt", 3);
    let hooks = profile.hooks();
    let id = hooks.list(None, 10).unwrap()[0].id;
    assert_eq!(
        hooks.restore(id).unwrap(),
        profile.at("home/a (restored).txt")
    );
    assert_eq!(
        profile.fs.metadata(&profile.at("home/a.txt")).unwrap().size,
        3
    );
    assert_eq!(
        profile
            .fs
            .metadata(&profile.at("home/a (restored).txt"))
            .unwrap()
            .size,
        8
    );
}

#[test]
fn restore_numbers_the_restored_name_when_it_is_taken_too() {
    let profile = Profile::new();
    profile.fs.add_file("home/a.txt", 8);
    profile.delete(&["home/a.txt"]);
    profile.fs.add_file("home/a.txt", 3);
    profile.fs.add_file("home/a (restored).txt", 4);
    let hooks = profile.hooks();
    let id = hooks.list(None, 10).unwrap()[0].id;
    assert_eq!(
        hooks.restore(id).unwrap(),
        profile.at("home/a (restored 2).txt")
    );
}

#[test]
fn restore_and_purge_of_a_missing_entry_refuse_with_a_message() {
    let profile = Profile::new();
    let hooks = profile.hooks();
    let missing = rusty_core::retention::RetentionId(42);
    let message = rusty_l10n::tr("retained-missing");
    assert_eq!(hooks.restore(missing), Err(message.clone()));
    assert_eq!(hooks.purge(missing), Err(message));
}

#[test]
fn purge_removes_the_entry_and_its_data() {
    let profile = Profile::new();
    profile.fs.add_file("home/a", 8);
    profile.delete(&["home/a"]);
    let hooks = profile.hooks();
    let id = hooks.list(None, 10).unwrap()[0].id;
    hooks.purge(id).unwrap();
    assert!(hooks.list(None, 10).unwrap().is_empty());
    assert_eq!(hooks.restore(id), Err(rusty_l10n::tr("retained-not-ready")));
}

#[test]
fn purge_of_a_pinned_entry_refuses_with_a_message_and_keeps_it() {
    let profile = Profile::new();
    profile.fs.add_file("home/a", 8);
    drop(profile.start_delete(&["home/a"]));
    let hooks = profile.hooks();
    let rows = hooks.list(None, 10).unwrap();
    assert_eq!(
        hooks.purge(rows[0].id),
        Err(rusty_l10n::tr("retained-busy"))
    );
    assert_eq!(
        hooks.restore(rows[0].id),
        Err(rusty_l10n::tr("retained-busy"))
    );
    assert_eq!(hooks.list(None, 10).unwrap(), rows);
}

#[test]
fn a_running_jobs_entry_lists_as_busy_without_waiting_for_its_lease() {
    let profile = Profile::new();
    profile.fs.add_file("home/a", 8);
    let running = profile.start_delete(&["home/a"]);
    let started = Instant::now();
    let rows = profile.hooks().list(None, 10).unwrap();
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].state, RetainedState::Busy);
    drop(running);
}

#[test]
fn safe_mode_or_no_profile_gives_no_retained_hooks() {
    let profile = Profile::new();
    profile.fs.add_file("home/a", 8);
    profile.delete(&["home/a"]);
    let off = hooks(Some(&profile.dirs), false, profile.fs.clone());
    assert!(off.list(None, 10).unwrap().is_empty());
    assert!(
        hooks(None, true, profile.fs.clone())
            .list(None, 10)
            .unwrap()
            .is_empty()
    );
}
