use std::ops::ControlFlow;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use rusty_core::rename::check::{Existing, ExistingName, Fold, Row};
use rusty_core::rename::plan_batch;
use rusty_core::{AppState, Command, FsError, FsErrorKind, Location, OperationPlan, Step};
use rusty_fs::{
    CancelToken, EntryBatch, FakeFs, FakeOp, Fault, HeldFile, JobLog, JobOutcome, Provider,
    ProviderOps, StepOutcome, is_rename_temporary,
};

use crate::browser::{
    Browser, JobStatus, JobView, OpLog, Operations, RenameHooks, RenamePresetList, UndoOffer,
};

mod logging;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Open,
    Done,
    Failed,
    Cancelled,
    RestorePending,
    Undone,
}

struct Record {
    state: State,
    plan: OperationPlan,
    done: Vec<u32>,
    undone: Vec<u32>,
    identities: std::collections::HashMap<u32, rusty_fs::FileIdentity>,
}

struct Book {
    jobs: Vec<Record>,
    gate: bool,
    cancel_after: Option<(usize, CancelToken)>,
    crash_on_undone: Option<u32>,
}

#[derive(Clone)]
struct Journal(Arc<Mutex<Book>>);

impl Journal {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(Book {
            jobs: Vec::new(),
            gate: true,
            cancel_after: None,
            crash_on_undone: None,
        })))
    }

    fn book(&self) -> MutexGuard<'_, Book> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn state(&self) -> State {
        self.book().jobs.last().map_or(State::Open, |job| job.state)
    }

    fn record(&self, job: u64) -> MutexGuard<'_, Book> {
        let book = self.book();
        assert!(index(job) < book.jobs.len(), "job {job}");
        book
    }

    fn state_of(&self, job: u64) -> State {
        self.record(job).jobs[index(job)].state
    }

    fn undone_of(&self, job: u64) -> Vec<u32> {
        let mut undone = self.record(job).jobs[index(job)].undone.clone();
        undone.sort_unstable();
        undone
    }

    fn add(&self, plan: OperationPlan, done: Vec<u32>, fs: &FakeFs) {
        let identities = (0_u32..)
            .zip(&plan.steps)
            .filter_map(|(seq, step)| {
                let Step::Rename { target, .. } = step else {
                    return None;
                };
                done.contains(&seq)
                    .then(|| {
                        fs.file_identity(target)
                            .unwrap()
                            .map(|identity| (seq, identity))
                    })
                    .flatten()
            })
            .collect();
        self.book().jobs.push(Record {
            state: State::Done,
            plan,
            done,
            undone: Vec::new(),
            identities,
        });
    }

    fn offer(&self) -> Option<UndoOffer> {
        let book = self.book();
        let (at, job) = book
            .jobs
            .iter()
            .enumerate()
            .rev()
            .find(|(_, job)| matches!(job.state, State::Done | State::RestorePending))?;
        Some(UndoOffer {
            job: u64::try_from(at + 1).unwrap(),
            plan: job.plan.clone(),
            done: job
                .done
                .iter()
                .copied()
                .filter(|seq| !job.undone.contains(seq))
                .collect(),
            created_at: 0,
            restore_pending: job.state == State::RestorePending,
        })
    }

    fn wait_for_gate(&self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.book().gate && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn hooks(&self) -> RenameHooks {
        let (last, undo) = (self.clone(), self.clone());
        RenameHooks {
            log: None,
            last: Arc::new(move || Ok(last.offer())),
            undo_log: Arc::new(move |offer: &UndoOffer| {
                undo.wait_for_gate();
                let job = index(offer.job);
                undo.book().jobs[job].state = State::RestorePending;
                Ok(Box::new(Log::new(&undo, job, true)) as _)
            }),
            presets: Arc::new(|| Ok(RenamePresetList::default())),
            save: Arc::new(|_, _| Ok(())),
            delete: Arc::new(|_| Ok(())),
        }
    }

    fn operations(&self) -> Operations {
        let journal = self.clone();
        Operations {
            retain_deletes: None,
            write: None,
            log: Arc::new(move |plan: &OperationPlan| {
                journal.wait_for_gate();
                let mut book = journal.book();
                book.jobs.push(Record {
                    state: State::Open,
                    plan: plan.clone(),
                    done: Vec::new(),
                    undone: Vec::new(),
                    identities: std::collections::HashMap::new(),
                });
                let job = book.jobs.len() - 1;
                drop(book);
                Ok(Box::new(Log::new(&journal, job, false)) as Box<dyn OpLog>)
            }),
            protected: Vec::new(),
        }
    }
}

fn index(job: u64) -> usize {
    usize::try_from(job).unwrap() - 1
}

struct Log {
    journal: Journal,
    job: usize,
    undoing: bool,
    put_back_failed: bool,
}

impl Log {
    fn new(journal: &Journal, job: usize, undoing: bool) -> Self {
        Self {
            journal: journal.clone(),
            job,
            undoing,
            put_back_failed: false,
        }
    }
}

impl JobLog for Log {
    fn prepare_rename(
        &mut self,
        seq: u32,
        identity: rusty_fs::FileIdentity,
    ) -> Result<(), FsError> {
        self.journal.book().jobs[self.job]
            .identities
            .insert(seq, identity);
        Ok(())
    }

    fn rename_identity(&self, seq: u32) -> Result<Option<rusty_fs::FileIdentity>, FsError> {
        Ok(self.journal.book().jobs[self.job]
            .identities
            .get(&seq)
            .copied())
    }

    fn step(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        let mut book = self.journal.book();
        if outcome == StepOutcome::Done {
            book.jobs[self.job].done.push(seq);
        }
        let done = book.jobs[self.job].done.len();
        if let Some((after, token)) = &book.cancel_after
            && done >= *after
        {
            token.cancel();
        }
        Ok(())
    }

    fn undone(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        let crash = self.journal.book().crash_on_undone == Some(seq);
        assert!(!crash, "crash before the journal write of step {seq}");
        if outcome == StepOutcome::Failed {
            self.put_back_failed = true;
        } else {
            self.journal.book().jobs[self.job].undone.push(seq);
        }
        Ok(())
    }

    fn journal_held(&mut self, _: &HeldFile) -> Result<bool, FsError> {
        Ok(false)
    }
}

impl OpLog for Log {
    fn finish(&mut self, outcome: &JobOutcome, _: &dyn ProviderOps) -> Result<(), String> {
        self.journal.book().jobs[self.job].state = match outcome {
            _ if self.put_back_failed => State::RestorePending,
            JobOutcome::Done if self.undoing => State::Undone,
            JobOutcome::Done => State::Done,
            _ if self.undoing => State::RestorePending,
            JobOutcome::Cancelled => State::Cancelled,
            JobOutcome::Failed { .. } => State::Failed,
        };
        Ok(())
    }
}

const NAMES: [&str; 5] = ["a", "b", "c", "d", "e"];
const SWAP_AND_RENAMES: [(&str, &str); 5] =
    [("a", "b"), ("b", "a"), ("c", "x"), ("d", "y"), ("e", "z")];
const SIMPLE: [(&str, &str); 3] = [("a", "x"), ("b", "y"), ("c", "z")];

#[test]
fn undo_leaves_a_replacement_with_matching_metadata_alone() {
    let fs = home(&["a"]);
    let journal = Journal::new();
    let mut browser = open(&fs, &journal);
    run(&mut browser, batch(&fs, &[("a", "x")]), None);
    let old = fs.metadata(&fs.location("home/x")).unwrap();
    fs.remove("home/x");
    fs.add_entry("home", old);
    undo(&mut browser, &journal);
    assert_eq!(listed(&browser), ["x"]);
    assert_eq!(last_job(&browser).report.skipped, 1);
    assert_eq!(last_job(&browser).status, JobStatus::Done);
}

#[test]
fn undo_keeps_an_in_place_edit_and_restores_its_original_name() {
    let fs = home(&["a"]);
    let journal = Journal::new();
    let mut browser = open(&fs, &journal);
    run(&mut browser, batch(&fs, &[("a", "x")]), None);
    fs.add_file("home/x", 99);
    undo(&mut browser, &journal);
    assert_eq!(listed(&browser), ["a"]);
    assert_eq!(size_of(&fs, "a"), Some(99));
    assert_eq!(last_job(&browser).report.skipped, 0);
}

#[test]
fn undo_skips_an_older_batch_without_recorded_identities() {
    let fs = home(&["a"]);
    let journal = Journal::new();
    let mut browser = open(&fs, &journal);
    run(&mut browser, batch(&fs, &[("a", "x")]), None);
    journal.book().jobs[0].identities.clear();
    undo(&mut browser, &journal);
    assert_eq!(listed(&browser), ["x"]);
    assert_eq!(last_job(&browser).report.skipped, 1);
}

#[test]
fn an_unreadable_identity_stops_undo_and_a_retry_finishes_it() {
    let fs = home(&["a"]);
    let journal = Journal::new();
    let mut browser = open(&fs, &journal);
    run(&mut browser, batch(&fs, &[("a", "x")]), None);
    fs.fault(
        "home/x",
        FakeOp::Identity,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    undo(&mut browser, &journal);
    assert_eq!(listed(&browser), ["x"]);
    assert_eq!(journal.state(), State::RestorePending);
    assert!(matches!(last_job(&browser).status, JobStatus::Failed(_)));
    undo(&mut browser, &journal);
    assert_eq!(listed(&browser), ["a"]);
    assert_eq!(journal.state(), State::Undone);
}

#[test]
fn an_unavailable_identity_keeps_undo_pending_until_it_can_be_checked() {
    let fs = home(&["a"]);
    let journal = Journal::new();
    let mut browser = open(&fs, &journal);
    run(&mut browser, batch(&fs, &[("a", "x")]), None);
    let volume = fs.volume_serial(&fs.location("home/x")).unwrap().unwrap();
    fs.set_volume_serial("home/x", None);
    undo(&mut browser, &journal);
    assert_eq!(listed(&browser), ["x"]);
    assert_eq!(journal.state(), State::RestorePending);
    fs.set_volume("home/x", volume);
    undo(&mut browser, &journal);
    assert_eq!(listed(&browser), ["a"]);
    assert_eq!(journal.state(), State::Undone);
}

#[test]
fn an_unproved_rename_runs_and_undoes_while_its_fingerprint_matches() {
    let fs = home(&["a"]);
    fs.set_volume_serial("home", None);
    let journal = Journal::new();
    let mut browser = open(&fs, &journal);
    run(&mut browser, batch(&fs, &[("a", "x")]), None);
    assert_eq!(last_job(&browser).status, JobStatus::Done);
    assert_eq!(listed(&browser), ["x"]);
    undo(&mut browser, &journal);
    assert_eq!(listed(&browser), ["a"]);
    assert_eq!(last_job(&browser).report.skipped, 0);
    assert_eq!(journal.state(), State::Undone);
}

#[test]
fn an_unproved_undo_reports_an_item_whose_size_or_time_changed() {
    for (size, modified_ms) in [(99, None), (10, Some(2_000))] {
        let fs = home(&["a"]);
        fs.set_volume_serial("home", None);
        let journal = Journal::new();
        let mut browser = open(&fs, &journal);
        run(&mut browser, batch(&fs, &[("a", "x")]), None);
        let mut entry = fs.metadata(&fs.location("home/x")).unwrap();
        entry.size = size;
        entry.modified_ms = modified_ms;
        fs.remove("home/x");
        fs.add_entry("home", entry);
        undo(&mut browser, &journal);
        assert_eq!(listed(&browser), ["x"]);
        assert_eq!(size_of(&fs, "x"), Some(size));
        assert_eq!(last_job(&browser).report.skipped, 1);
    }
}

fn identical_home(names: &[&str]) -> Arc<FakeFs> {
    let fs = home(names);
    for name in names {
        let path = format!("home/{name}");
        let mut entry = fs.metadata(&fs.location(&path)).unwrap();
        entry.size = 10;
        entry.modified_ms = Some(5_000_000);
        fs.remove(&path);
        fs.add_entry("home", entry);
    }
    fs
}

fn inodes(fs: &FakeFs, names: &[&str]) -> Vec<Option<[u8; 16]>> {
    names
        .iter()
        .map(|name| {
            let identity = fs.file_identity(&fs.location(&format!("home/{name}")));
            identity.ok().flatten().map(|identity| identity.file)
        })
        .collect()
}

#[test]
fn undo_after_a_fault_returns_swaps_and_cycles_of_identical_unproved_items() {
    let cases: [&[(&str, &str)]; 2] = [
        &[("a", "b"), ("b", "a")],
        &[("a", "b"), ("b", "c"), ("c", "a")],
    ];
    for pairs in cases {
        let fs = identical_home(&["a", "b", "c"]);
        let original = inodes(&fs, &["a", "b", "c"]);
        fs.set_volume_serial("home", None);
        let journal = Journal::new();
        let mut browser = open(&fs, &journal);
        let plan = batch(&fs, pairs);
        let middle = target_of(&plan, 1);
        run(&mut browser, plan, None);
        assert_eq!(last_job(&browser).status, JobStatus::Done, "{pairs:?}");
        let next = fs.calls(&middle, FakeOp::Rename) + 1;
        fs.fault(
            &middle,
            FakeOp::Rename,
            next,
            Fault::Fail(FsErrorKind::Locked),
        );
        undo(&mut browser, &journal);
        assert_eq!(journal.state(), State::RestorePending, "{pairs:?}");
        undo(&mut browser, &journal);
        assert_eq!(last_job(&browser).status, JobStatus::Done, "{pairs:?}");
        assert_eq!(last_job(&browser).report.skipped, 0, "{pairs:?}");
        assert_eq!(journal.state(), State::Undone, "{pairs:?}");
        assert_eq!(listed(&browser), ["a", "b", "c"], "{pairs:?}");
        fs.set_volume_serial("home", Some(1));
        let restored = inodes(&fs, &["a", "b", "c"]);
        assert_eq!(restored, original, "{pairs:?}");
    }
}

fn home(names: &[&str]) -> Arc<FakeFs> {
    let fs = FakeFs::new();
    fs.add_folder("home");
    for (size, name) in (1_u64..).zip(names) {
        fs.add_file(&format!("home/{name}"), size * 10);
    }
    Arc::new(fs)
}

fn batch(fs: &FakeFs, pairs: &[(&str, &str)]) -> OperationPlan {
    let rows: Vec<Row> = pairs
        .iter()
        .map(|(old, new)| Row {
            parent: fs.location("home"),
            old: (*old).to_owned(),
            new: (*new).to_owned(),
            excluded: false,
        })
        .collect();
    let existing = Existing::new(
        names_in(fs)
            .into_iter()
            .map(|name| ExistingName {
                parent: fs.location("home"),
                name,
            })
            .collect(),
    );
    plan_batch(&rows, &existing, Fold::Exact, 7)
        .plan
        .expect("a plan")
}

fn names_in(fs: &FakeFs) -> Vec<String> {
    let mut names = Vec::new();
    let mut sink = |batch: EntryBatch| {
        names.extend(batch.entries.into_iter().map(|entry| entry.name.to_text()));
        ControlFlow::Continue(())
    };
    let _ = fs.list(&fs.location("home"), &mut sink, &CancelToken::new());
    names.sort();
    names
}

fn source_of(plan: &OperationPlan, seq: usize) -> String {
    match &plan.steps[seq] {
        Step::Rename { source, .. } => source.path().to_text(),
        other => panic!("unexpected step {other:?}"),
    }
}

fn target_of(plan: &OperationPlan, seq: usize) -> String {
    match &plan.steps[seq] {
        Step::Rename { target, .. } => target.path().to_text(),
        other => panic!("unexpected step {other:?}"),
    }
}

fn open(fs: &Arc<FakeFs>, journal: &Journal) -> Browser {
    let provider: Arc<dyn ProviderOps> = fs.clone();
    let mut browser = Browser::new(
        AppState::new(fs.location("home")),
        provider,
        Arc::new(|| {}),
    );
    browser.set_operations(journal.operations());
    browser.set_rename_hooks(Some(journal.hooks()));
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

fn listed(browser: &Browser) -> Vec<String> {
    let mut names: Vec<String> = browser
        .state()
        .active_tab()
        .listing()
        .map(|l| l.entries().iter().map(|e| e.name.clone()).collect())
        .unwrap_or_default();
    names.sort();
    names
}

fn size_of(fs: &FakeFs, name: &str) -> Option<u64> {
    fs.metadata(&fs.location(&format!("home/{name}")))
        .ok()
        .map(|entry| entry.size)
}

fn sizes(fs: &FakeFs, names: &[&str]) -> Vec<Option<u64>> {
    names.iter().map(|name| size_of(fs, name)).collect()
}

fn last_job(browser: &Browser) -> &JobView {
    browser.jobs().last().expect("a job")
}

fn run(browser: &mut Browser, plan: OperationPlan, first: Option<&str>) {
    assert!(browser.rename_batch(plan, first));
    settle(browser);
}

fn undo(browser: &mut Browser, journal: &Journal) {
    let offer = journal.offer().expect("an offer");
    assert!(browser.undo_rename(offer));
    settle(browser);
}

fn mark_all(browser: &mut Browser, count: usize) {
    for _ in 0..count {
        browser.command(&Command::SelectionToggleDown).unwrap();
    }
}

fn marked(browser: &Browser) -> usize {
    browser.state().active_tab().selection().marked().len()
}

#[test]
fn five_items_with_a_swap_rename_correctly() {
    let fs = home(&NAMES);
    let journal = Journal::new();
    let mut browser = open(&fs, &journal);
    let plan = batch(&fs, &SWAP_AND_RENAMES);
    run(&mut browser, plan, Some("x"));
    assert_eq!(last_job(&browser).status, JobStatus::Done);
    assert_eq!(
        sizes(&fs, &["a", "b", "x", "y", "z", "c"]),
        [Some(20), Some(10), Some(30), Some(40), Some(50), None]
    );
    assert_eq!(listed(&browser), ["a", "b", "x", "y", "z"]);
    assert_eq!(journal.state(), State::Done);
}

#[test]
fn done_clears_the_marks_and_puts_the_cursor_on_the_first_new_name() {
    let fs = home(&NAMES);
    let journal = Journal::new();
    let mut browser = open(&fs, &journal);
    mark_all(&mut browser, 3);
    let plan = batch(&fs, &SIMPLE);
    run(&mut browser, plan, Some("z"));
    settle(&mut browser);
    let tab = browser.state().active_tab();
    let names: Vec<String> = tab
        .listing()
        .unwrap()
        .entries()
        .iter()
        .map(|e| e.name.clone())
        .collect();
    assert_eq!(marked(&browser), 0);
    let cursor = usize::try_from(tab.view().cursor).unwrap();
    assert_eq!(names[cursor], "z");
}

#[test]
fn a_fault_at_any_step_puts_every_name_back_and_keeps_the_marks() {
    for step in 0..SIMPLE.len() {
        let fs = home(&NAMES);
        let journal = Journal::new();
        let mut browser = open(&fs, &journal);
        mark_all(&mut browser, 3);
        let plan = batch(&fs, &SIMPLE);
        fs.fault(
            &source_of(&plan, step),
            FakeOp::Rename,
            1,
            Fault::Fail(FsErrorKind::Locked),
        );
        run(&mut browser, plan, Some("x"));
        assert!(
            matches!(last_job(&browser).status, JobStatus::Failed(_)),
            "step {step}"
        );
        assert_eq!(listed(&browser), NAMES, "step {step}");
        assert_eq!(
            sizes(&fs, &NAMES),
            [Some(10), Some(20), Some(30), Some(40), Some(50)]
        );
        assert_eq!(marked(&browser), 3, "step {step}");
        assert_eq!(journal.state(), State::Failed, "step {step}");
        assert!(journal.offer().is_none(), "step {step}");
    }
}

#[test]
fn a_fault_inside_a_swap_puts_the_swap_back() {
    for step in 0..3 {
        let fs = home(&NAMES);
        let journal = Journal::new();
        let mut browser = open(&fs, &journal);
        let plan = batch(&fs, &SWAP_AND_RENAMES);
        fs.fault(
            &source_of(&plan, step),
            FakeOp::Rename,
            1,
            Fault::Fail(FsErrorKind::AccessDenied),
        );
        run(&mut browser, plan, None);
        assert_eq!(listed(&browser), NAMES, "step {step}");
        assert_eq!(
            sizes(&fs, &NAMES),
            [Some(10), Some(20), Some(30), Some(40), Some(50)],
            "step {step}"
        );
    }
}

#[test]
fn a_cancel_in_the_middle_puts_every_name_back() {
    let fs = home(&NAMES);
    let journal = Journal::new();
    let mut browser = open(&fs, &journal);
    mark_all(&mut browser, 3);
    journal.book().gate = false;
    let plan = batch(&fs, &SIMPLE);
    assert!(browser.rename_batch(plan, Some("x")));
    let token = last_job(&browser).cancel.clone();
    journal.book().cancel_after = Some((2, token));
    journal.book().gate = true;
    settle(&mut browser);
    assert_eq!(last_job(&browser).status, JobStatus::Cancelled);
    assert_eq!(listed(&browser), NAMES);
    assert_eq!(
        sizes(&fs, &NAMES),
        [Some(10), Some(20), Some(30), Some(40), Some(50)]
    );
    assert_eq!(marked(&browser), 3);
    assert_eq!(journal.state(), State::Cancelled);
    assert!(journal.offer().is_none());
}

fn stuck_batch(fs: &Arc<FakeFs>, journal: &Journal) -> (Browser, OperationPlan) {
    let mut browser = open(fs, journal);
    mark_all(&mut browser, 3);
    let plan = batch(fs, &SIMPLE);
    fs.fault(
        &source_of(&plan, 2),
        FakeOp::Rename,
        1,
        Fault::Fail(FsErrorKind::Locked),
    );
    fs.fault(
        &target_of(&plan, 1),
        FakeOp::Rename,
        1,
        Fault::Fail(FsErrorKind::Locked),
    );
    run(&mut browser, plan.clone(), Some("x"));
    (browser, plan)
}

#[test]
fn a_stuck_put_back_stops_there_is_restore_pending_and_lists_the_files_left() {
    let fs = home(&NAMES);
    let journal = Journal::new();
    let (browser, plan) = stuck_batch(&fs, &journal);
    let job = last_job(&browser);
    assert!(matches!(job.status, JobStatus::Failed(_)));
    assert_eq!(
        job.report.left,
        [target_of(&plan, 0), target_of(&plan, 1)].map(|at| fs.location(&at))
    );
    assert_eq!(listed(&browser), ["c", "d", "e", "x", "y"]);
    assert_eq!(marked(&browser), 1);
    assert_eq!(journal.state(), State::RestorePending);
    assert_eq!(journal.offer().expect("still undoable").done, [0, 1]);
}

#[test]
fn the_next_undo_finishes_a_stuck_put_back() {
    let fs = home(&NAMES);
    let journal = Journal::new();
    let (mut browser, _) = stuck_batch(&fs, &journal);
    undo(&mut browser, &journal);
    assert_eq!(last_job(&browser).status, JobStatus::Done);
    assert_eq!(listed(&browser), NAMES);
    assert_eq!(
        sizes(&fs, &NAMES),
        [Some(10), Some(20), Some(30), Some(40), Some(50)]
    );
    assert_eq!(journal.state(), State::Undone);
    assert!(journal.offer().is_none());
}

#[test]
fn undo_of_a_done_batch_restores_every_name() {
    let fs = home(&NAMES);
    let journal = Journal::new();
    let mut browser = open(&fs, &journal);
    let plan = batch(&fs, &SWAP_AND_RENAMES);
    run(&mut browser, plan, Some("x"));
    undo(&mut browser, &journal);
    let job = last_job(&browser);
    assert_eq!(job.status, JobStatus::Done);
    assert!(job.undo);
    assert_eq!((job.progress.files_done, job.progress.files_total), (5, 5));
    assert_eq!(job.report.skipped, 0);
    assert_eq!(listed(&browser), NAMES);
    assert_eq!(
        sizes(&fs, &NAMES),
        [Some(10), Some(20), Some(30), Some(40), Some(50)]
    );
    assert_eq!(journal.state(), State::Undone);
    assert!(journal.offer().is_none());
}

#[test]
fn undo_skips_an_item_renamed_since_the_batch() {
    let fs = home(&NAMES);
    let journal = Journal::new();
    let mut browser = open(&fs, &journal);
    let plan = batch(&fs, &SIMPLE);
    run(&mut browser, plan, None);
    fs.rename_entry("home/y", "home/other");
    undo(&mut browser, &journal);
    let job = last_job(&browser);
    assert_eq!(job.status, JobStatus::Done);
    assert_eq!(job.report.skipped, 1);
    assert_eq!(listed(&browser), ["a", "c", "d", "e", "other"]);
    assert_eq!(size_of(&fs, "other"), Some(20));
}

#[test]
fn undo_skips_an_item_whose_old_name_was_taken_since_the_batch() {
    let fs = home(&NAMES);
    let journal = Journal::new();
    let mut browser = open(&fs, &journal);
    let plan = batch(&fs, &SIMPLE);
    run(&mut browser, plan, None);
    fs.add_file("home/b", 99);
    undo(&mut browser, &journal);
    let job = last_job(&browser);
    assert_eq!(job.report.skipped, 1);
    assert_eq!(listed(&browser), ["a", "b", "c", "d", "e", "y"]);
    assert_eq!(
        sizes(&fs, &["a", "b", "y", "c"]),
        [Some(10), Some(99), Some(20), Some(30)]
    );
}

#[test]
fn undo_of_a_swap_whose_item_was_deleted_keeps_the_survivor_right() {
    let fs = home(&["a", "b"]);
    let journal = Journal::new();
    let mut browser = open(&fs, &journal);
    let plan = batch(&fs, &[("a", "b"), ("b", "a")]);
    run(&mut browser, plan, None);
    fs.remove("home/b");
    undo(&mut browser, &journal);
    assert_eq!(listed(&browser), ["b"]);
    assert_eq!(size_of(&fs, "b"), Some(20));
}

#[test]
fn a_failed_undo_lists_the_item_left_under_a_temporary_name() {
    let fs = home(&["a", "b"]);
    let journal = Journal::new();
    let mut browser = open(&fs, &journal);
    let plan = batch(&fs, &[("a", "b"), ("b", "a")]);
    let temporary = target_of(&plan, 0);
    run(&mut browser, plan, None);
    fs.fault(
        &temporary,
        FakeOp::Rename,
        2,
        Fault::Fail(FsErrorKind::Locked),
    );
    undo(&mut browser, &journal);
    let job = last_job(&browser);
    assert!(matches!(job.status, JobStatus::Failed(_)));
    assert_eq!(job.report.left, [fs.location(&temporary)]);
    assert_ne!(journal.state(), State::Undone);
}

#[test]
fn a_second_undo_waits_while_one_is_running() {
    let fs = home(&NAMES);
    let journal = Journal::new();
    let mut browser = open(&fs, &journal);
    let plan = batch(&fs, &SIMPLE);
    run(&mut browser, plan, None);
    let offer = journal.offer().unwrap();
    assert!(browser.undo_rename(offer.clone()));
    assert!(!browser.undo_rename(offer));
    settle(&mut browser);
}

#[test]
fn undo_without_hooks_does_nothing() {
    let fs = home(&NAMES);
    let provider: Arc<dyn ProviderOps> = fs.clone();
    let mut browser = Browser::new(
        AppState::new(fs.location("home")),
        provider,
        Arc::new(|| {}),
    );
    browser.start();
    settle(&mut browser);
    let plan = batch(&fs, &SIMPLE);
    let offer = UndoOffer {
        job: 1,
        plan,
        done: vec![0, 1, 2],
        created_at: 0,
        restore_pending: false,
    };
    assert!(!browser.undo_rename(offer));
    assert_eq!(listed(&browser), NAMES);
}

#[test]
fn a_plan_that_is_not_a_rename_batch_is_refused() {
    let fs = home(&NAMES);
    let journal = Journal::new();
    let mut browser = open(&fs, &journal);
    let mut plan = batch(&fs, &SIMPLE);
    plan.steps.clear();
    assert!(!browser.rename_batch(plan, None));
    let mut copy = batch(&fs, &SIMPLE);
    copy.kind = rusty_core::OpKind::Copy;
    assert!(!browser.rename_batch(copy, None));
    assert!(browser.jobs().is_empty());
}

#[test]
fn the_hooks_are_kept_with_the_browser() {
    let fs = home(&NAMES);
    let journal = Journal::new();
    let browser = open(&fs, &journal);
    let hooks = browser.rename_hooks().expect("hooks");
    assert!(((hooks.save)("mine", "{}")).is_ok());
    assert_eq!((hooks.presets)().unwrap(), RenamePresetList::default());
}

#[test]
fn undo_journals_a_skipped_item_and_the_next_offer_is_the_older_batch() {
    let fs = home(&NAMES);
    let journal = Journal::new();
    let mut browser = open(&fs, &journal);
    run(&mut browser, batch(&fs, &[("c", "z")]), None);
    run(&mut browser, batch(&fs, &[("a", "x"), ("b", "y")]), None);
    fs.rename_entry("home/y", "home/other");
    undo(&mut browser, &journal);
    let job = last_job(&browser);
    assert_eq!(
        (job.status.clone(), job.report.skipped),
        (JobStatus::Done, 1)
    );
    assert_eq!(listed(&browser), ["a", "d", "e", "other", "z"]);
    assert_eq!(journal.undone_of(2), [0, 1]);
    assert_eq!(journal.state_of(2), State::Undone);
    assert_eq!(journal.offer().expect("the older batch").job, 1);
}

#[test]
fn a_failed_check_of_an_item_fails_the_undo_and_keeps_it_pending() {
    for kind in [FsErrorKind::AccessDenied, FsErrorKind::Unknown] {
        let fs = home(&NAMES);
        let journal = Journal::new();
        let mut browser = open(&fs, &journal);
        run(&mut browser, batch(&fs, &SIMPLE), None);
        let next = fs.calls("home/y", FakeOp::Metadata) + 1;
        fs.fault("home/y", FakeOp::Metadata, next, Fault::Fail(kind));
        undo(&mut browser, &journal);
        let job = last_job(&browser);
        assert!(matches!(job.status, JobStatus::Failed(_)), "{kind:?}");
        assert_eq!(job.report.skipped, 0, "{kind:?}");
        assert_eq!(journal.state(), State::RestorePending, "{kind:?}");
        assert_eq!(
            journal.offer().expect("still undoable").done,
            [0, 1],
            "{kind:?}"
        );
        undo(&mut browser, &journal);
        assert_eq!(listed(&browser), NAMES, "{kind:?}");
        assert_eq!(journal.state(), State::Undone, "{kind:?}");
    }
}

#[test]
fn a_crash_during_undo_leaves_the_batch_restore_pending_and_undo_finishes_it() {
    let fs = home(&NAMES);
    let journal = Journal::new();
    let mut browser = open(&fs, &journal);
    run(&mut browser, batch(&fs, &SIMPLE), None);
    journal.book().crash_on_undone = Some(1);
    undo(&mut browser, &journal);
    assert!(matches!(last_job(&browser).status, JobStatus::Failed(_)));
    assert_eq!(journal.state(), State::RestorePending);
    journal.book().crash_on_undone = None;
    undo(&mut browser, &journal);
    assert_eq!(listed(&browser), NAMES);
    assert_eq!(journal.state(), State::Undone);
}

#[test]
fn a_rename_batch_and_an_undo_wait_while_a_batch_runs() {
    let fs = home(&NAMES);
    let journal = Journal::new();
    let mut browser = open(&fs, &journal);
    run(&mut browser, batch(&fs, &[("e", "z")]), None);
    let offer = journal.offer().unwrap();
    journal.book().gate = false;
    assert!(browser.rename_batch(batch(&fs, &[("a", "x")]), None));
    assert!(!browser.rename_batch(batch(&fs, &[("b", "y")]), None));
    assert!(!browser.undo_rename(offer));
    journal.book().gate = true;
    settle(&mut browser);
    assert_eq!(listed(&browser), ["b", "c", "d", "x", "z"]);
}

#[test]
fn a_rename_batch_waits_while_an_undo_runs() {
    let fs = home(&NAMES);
    let journal = Journal::new();
    let mut browser = open(&fs, &journal);
    run(&mut browser, batch(&fs, &[("a", "x")]), None);
    let offer = journal.offer().unwrap();
    journal.book().gate = false;
    assert!(browser.undo_rename(offer));
    assert!(!browser.rename_batch(batch(&fs, &[("b", "y")]), None));
    journal.book().gate = true;
    settle(&mut browser);
    assert_eq!(listed(&browser), NAMES);
}

#[test]
fn an_undo_job_cannot_be_cancelled_or_paused() {
    let fs = home(&NAMES);
    let journal = Journal::new();
    let mut browser = open(&fs, &journal);
    run(&mut browser, batch(&fs, &SIMPLE), None);
    let offer = journal.offer().unwrap();
    journal.book().gate = false;
    assert!(browser.undo_rename(offer));
    let id = last_job(&browser).id;
    assert!(!browser.cancel_job(id));
    assert!(!browser.pause_job(id));
    journal.book().gate = true;
    settle(&mut browser);
    assert_eq!(last_job(&browser).status, JobStatus::Done);
    assert_eq!(listed(&browser), NAMES);
}

#[test]
fn undo_lists_a_temporary_name_left_by_a_step_that_was_never_journaled() {
    let fs = home(&["a", "b", "c"]);
    let journal = Journal::new();
    let mut browser = open(&fs, &journal);
    let plan = batch(&fs, &[("a", "b"), ("b", "a"), ("c", "x")]);
    let seq_of = |found: &dyn Fn(&Location) -> bool| {
        (0..plan.steps.len())
            .find(|&seq| matches!(&plan.steps[seq], Step::Rename { target, .. } if found(target)))
            .unwrap()
    };
    let to_x = seq_of(&|target| *target == fs.location("home/x"));
    let to_temporary = seq_of(&|target| is_rename_temporary(target));
    let temporary = target_of(&plan, to_temporary);
    fs.rename_entry("home/c", "home/x");
    fs.rename_entry(&source_of(&plan, to_temporary), &temporary);
    journal.add(plan, vec![u32::try_from(to_x).unwrap()], &fs);
    undo(&mut browser, &journal);
    let job = last_job(&browser);
    assert_eq!(job.status, JobStatus::Done);
    assert_eq!(job.report.left, [fs.location(&temporary)]);
    assert_eq!(size_of(&fs, "c"), Some(30));
}

#[test]
fn an_undo_keeps_the_marks() {
    let fs = home(&NAMES);
    let journal = Journal::new();
    let mut browser = open(&fs, &journal);
    run(&mut browser, batch(&fs, &SIMPLE), None);
    mark_all(&mut browser, 2);
    undo(&mut browser, &journal);
    assert_eq!(listed(&browser), NAMES);
    assert_eq!(marked(&browser), 2);
}

fn swapped(fs: &Arc<FakeFs>, journal: &Journal) -> (Browser, String) {
    let mut browser = open(fs, journal);
    let plan = batch(fs, &[("a", "b"), ("b", "a")]);
    assert_eq!(
        (source_of(&plan, 1), target_of(&plan, 1)),
        ("home/b".to_owned(), "home/a".to_owned())
    );
    let temporary = target_of(&plan, 0);
    assert!(is_rename_temporary(&fs.location(&temporary)));
    run(&mut browser, plan, None);
    (browser, temporary)
}

#[test]
fn a_failed_undo_step_stops_the_undo_and_the_retry_finishes_the_swap() {
    let fs = home(&["a", "b"]);
    let journal = Journal::new();
    let (mut browser, temporary) = swapped(&fs, &journal);
    let next = fs.calls("home/a", FakeOp::Rename) + 1;
    fs.fault(
        "home/a",
        FakeOp::Rename,
        next,
        Fault::Fail(FsErrorKind::Locked),
    );
    undo(&mut browser, &journal);
    assert!(matches!(last_job(&browser).status, JobStatus::Failed(_)));
    assert_eq!(journal.undone_of(1), [2]);
    assert_eq!(journal.state(), State::RestorePending);
    assert_eq!(journal.offer().expect("still undoable").done, [0, 1]);
    undo(&mut browser, &journal);
    assert_eq!(last_job(&browser).status, JobStatus::Done);
    assert_eq!(sizes(&fs, &["a", "b"]), [Some(10), Some(20)]);
    assert_eq!(size_of(&fs, &temporary), None);
    assert_eq!(journal.state(), State::Undone);
}

#[test]
fn a_pending_undo_left_only_under_a_temporary_name_is_retried() {
    let fs = home(&["a", "b"]);
    let journal = Journal::new();
    let (mut browser, temporary) = swapped(&fs, &journal);
    let next = fs.calls(&temporary, FakeOp::Rename) + 1;
    fs.fault(
        &temporary,
        FakeOp::Rename,
        next,
        Fault::Fail(FsErrorKind::Locked),
    );
    undo(&mut browser, &journal);
    assert_eq!(last_job(&browser).report.left, [fs.location(&temporary)]);
    let offer = journal.offer().expect("still undoable");
    assert_eq!((offer.items(), offer.done.as_slice()), (0, [0].as_slice()));
    assert!(browser.undo_rename(offer));
    settle(&mut browser);
    assert_eq!(last_job(&browser).status, JobStatus::Done);
    assert_eq!(sizes(&fs, &["a", "b"]), [Some(10), Some(20)]);
    assert_eq!(journal.state(), State::Undone);
}

#[test]
fn an_unreadable_temporary_name_after_undo_is_listed_and_fails_the_undo() {
    for kind in [FsErrorKind::AccessDenied, FsErrorKind::Unknown] {
        let fs = home(&["a", "b"]);
        let journal = Journal::new();
        let (mut browser, temporary) = swapped(&fs, &journal);
        let scan = fs.calls(&temporary, FakeOp::Metadata) + 3;
        fs.fault(&temporary, FakeOp::Metadata, scan, Fault::Fail(kind));
        undo(&mut browser, &journal);
        let job = last_job(&browser);
        assert!(matches!(job.status, JobStatus::Failed(_)), "{kind:?}");
        assert_eq!(job.report.left, [fs.location(&temporary)], "{kind:?}");
        assert_eq!(sizes(&fs, &["a", "b"]), [Some(10), Some(20)], "{kind:?}");
        assert_eq!(journal.state(), State::RestorePending, "{kind:?}");
        undo(&mut browser, &journal);
        assert_eq!(last_job(&browser).status, JobStatus::Done, "{kind:?}");
        assert_eq!(journal.state(), State::Undone, "{kind:?}");
    }
}

#[test]
fn opus_swap_undo_preserves_a_replaced_member_and_keeps_the_job_restore_pending() {
    let fs = home(&["a", "b"]);
    let journal = Journal::new();
    let mut browser = open(&fs, &journal);
    let plan = batch(&fs, &[("a", "b"), ("b", "a")]);
    run(&mut browser, plan, None);
    fs.remove("home/b");
    fs.add_file("home/b", 99);
    undo(&mut browser, &journal);
    assert_eq!(size_of(&fs, "b"), Some(99));
    assert_eq!(size_of(&fs, "a"), Some(20));
    assert_eq!(journal.state(), State::RestorePending);
}
