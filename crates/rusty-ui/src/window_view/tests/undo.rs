use std::ops::ControlFlow;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use rusty_core::rename::check::{Existing, ExistingName, Fold, Row};
use rusty_core::rename::plan_batch;
use rusty_core::{AppState, FsError, OpKind, OperationPlan, Step};
use rusty_fs::{
    CancelToken, EntryBatch, FakeFs, HeldFile, JobLog, JobOutcome, Provider, ProviderOps,
    StepOutcome,
};

use super::{options_for, press, settle};
use crate::activity::ActivityKind;
use crate::browser::{OpLog, Operations, RenameHooks, RenamePresetList, UndoOffer};
use crate::window_view::FerriteWindow;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Open,
    Done,
    Undone,
    Pending,
}

struct Record {
    state: State,
    plan: OperationPlan,
    done: Vec<u32>,
    undone: Vec<u32>,
    identities: std::collections::HashMap<u32, rusty_fs::FileIdentity>,
}

#[derive(Clone, Default)]
struct Journal(Arc<Mutex<Vec<Record>>>);

impl Journal {
    fn book(&self) -> MutexGuard<'_, Vec<Record>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn state(&self) -> Option<State> {
        self.book().last().map(|record| record.state)
    }

    fn add(&self, plan: OperationPlan, (done, state): (Vec<u32>, State), fs: &FakeFs) {
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
        self.book().push(Record {
            state,
            plan,
            done,
            undone: Vec::new(),
            identities,
        });
    }

    fn offer(&self) -> Option<UndoOffer> {
        let book = self.book();
        let (at, record) = book
            .iter()
            .enumerate()
            .rev()
            .find(|(_, record)| matches!(record.state, State::Done | State::Pending))?;
        Some(UndoOffer {
            job: u64::try_from(at + 1).ok()?,
            plan: record.plan.clone(),
            done: record
                .done
                .iter()
                .copied()
                .filter(|seq| !record.undone.contains(seq))
                .collect(),
            created_at: 0,
            restore_pending: record.state == State::Pending,
        })
    }

    fn hooks(&self) -> RenameHooks {
        let (last, undo) = (self.clone(), self.clone());
        RenameHooks {
            log: None,
            last: Arc::new(move || Ok(last.offer())),
            undo_log: Arc::new(move |offer: &UndoOffer| {
                let job = usize::try_from(offer.job).map_err(|e| e.to_string())? - 1;
                undo.book()[job].state = State::Pending;
                Ok(Box::new(Log::new(&undo, job, true)) as Box<dyn OpLog>)
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
                let job = {
                    let mut book = journal.book();
                    book.push(Record {
                        state: State::Open,
                        plan: plan.clone(),
                        done: Vec::new(),
                        undone: Vec::new(),
                        identities: std::collections::HashMap::new(),
                    });
                    book.len() - 1
                };
                Ok(Box::new(Log::new(&journal, job, false)) as Box<dyn OpLog>)
            }),
            protected: Vec::new(),
        }
    }
}

struct Log {
    journal: Journal,
    job: usize,
    undoing: bool,
}

impl Log {
    fn new(journal: &Journal, job: usize, undoing: bool) -> Self {
        Self {
            journal: journal.clone(),
            job,
            undoing,
        }
    }
}

impl JobLog for Log {
    fn prepare_rename(
        &mut self,
        seq: u32,
        identity: rusty_fs::FileIdentity,
    ) -> Result<(), FsError> {
        self.journal.book()[self.job]
            .identities
            .insert(seq, identity);
        Ok(())
    }

    fn rename_identity(&self, seq: u32) -> Result<Option<rusty_fs::FileIdentity>, FsError> {
        Ok(self.journal.book()[self.job].identities.get(&seq).copied())
    }

    fn step(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        if outcome == StepOutcome::Done {
            self.journal.book()[self.job].done.push(seq);
        }
        Ok(())
    }

    fn undone(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        if outcome != StepOutcome::Failed {
            self.journal.book()[self.job].undone.push(seq);
        }
        Ok(())
    }

    fn journal_held(&mut self, _: &HeldFile) -> Result<bool, FsError> {
        Ok(false)
    }
}

impl OpLog for Log {
    fn finish(&mut self, outcome: &JobOutcome, _: &dyn ProviderOps) -> Result<(), String> {
        let state = match (outcome, self.undoing) {
            (JobOutcome::Done, true) => State::Undone,
            (JobOutcome::Done, false) => State::Done,
            (_, true) => State::Pending,
            _ => State::Open,
        };
        self.journal.book()[self.job].state = state;
        Ok(())
    }
}

struct Rig {
    window: AnyWindowHandle,
    view: Entity<FerriteWindow>,
}

fn files() -> Arc<FakeFs> {
    let fs = FakeFs::new();
    fs.add_folder("home/docs");
    for name in ["a.txt", "b.txt", "c.txt", "d.txt"] {
        fs.add_file(&format!("home/{name}"), 1);
    }
    Arc::new(fs)
}

fn open(cx: &mut TestAppContext, fs: Arc<FakeFs>, journal: &Journal) -> Rig {
    open_with(cx, fs, journal.operations(), journal.hooks())
}

fn open_with(
    cx: &mut TestAppContext,
    fs: Arc<FakeFs>,
    operations: Operations,
    hooks: RenameHooks,
) -> Rig {
    let mut options = options_for(fs.clone(), AppState::new(fs.location("home")));
    options.operations = operations;
    options.rename = Some(hooks);
    let (window, view) = super::open_options(cx, options);
    Rig { window, view }
}

impl Rig {
    fn press(&self, cx: &mut TestAppContext, keys: &[&str]) {
        for key in keys {
            press(cx, self.window, &self.view, key);
            self.wait(cx, Duration::from_millis(200));
        }
    }

    fn wait(&self, cx: &mut TestAppContext, time: Duration) {
        cx.executor().advance_clock(time);
        cx.run_until_parked();
        settle(cx, &self.view);
    }

    fn type_text(&self, cx: &mut TestAppContext, text: &str) {
        cx.update_window(self.window, |_, window, cx| window.input(text, cx))
            .expect("window");
        self.wait(cx, Duration::from_millis(200));
    }

    fn rename_three(&self, cx: &mut TestAppContext) {
        self.press(cx, &["down", "shift-down", "shift-down", "ctrl-m"]);
        self.type_text(cx, "x[N]");
        self.press(cx, &["enter"]);
    }

    fn listing(&self, cx: &mut TestAppContext) -> Vec<String> {
        self.view.read_with(cx, |view, _| {
            let tab = view.browser.state().active_tab();
            tab.listing()
                .map(|listing| listing.entries().iter().map(|e| e.name.clone()).collect())
                .unwrap_or_default()
        })
    }

    fn is_asking(&self, cx: &mut TestAppContext) -> bool {
        self.view.read_with(cx, |view, _| {
            view.browser
                .pending()
                .is_some_and(|op| op.undo.is_some() && op.plan.kind == OpKind::Rename)
        })
    }

    fn notice(&self, cx: &mut TestAppContext) -> Option<String> {
        self.view.read_with(cx, |view, _| {
            view.status_notice().map(|n| n.text.to_string())
        })
    }

    fn done_detail(&self, cx: &mut TestAppContext) -> Option<String> {
        self.view.read_with(cx, |view, _| {
            let items = &view.job_ui.strip.items;
            let done = items.iter().find(|item| item.kind == ActivityKind::Done);
            done.and_then(|item| item.detail.as_ref().map(ToString::to_string))
        })
    }

    fn skipped(&self, cx: &mut TestAppContext) -> usize {
        self.view.read_with(cx, |view, _| {
            view.browser
                .jobs()
                .last()
                .map_or(0, |job| job.report.skipped)
        })
    }
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| (*name).to_owned()).collect()
}

const RENAMED: [&str; 5] = ["docs", "d.txt", "xa.txt", "xb.txt", "xc.txt"];
const ORIGINAL: [&str; 5] = ["docs", "a.txt", "b.txt", "c.txt", "d.txt"];

#[gpui_kit::test]
fn f2_on_one_file_can_be_undone_with_ctrl_z(cx: &mut TestAppContext) {
    let journal = Journal::default();
    let rig = open(cx, files(), &journal);
    rig.press(cx, &["down", "f2", "ctrl-a"]);
    rig.type_text(cx, "renamed.txt");
    rig.press(cx, &["enter"]);
    assert!(rig.listing(cx).contains(&"renamed.txt".to_owned()));
    rig.press(cx, &["ctrl-z"]);
    if rig.is_asking(cx) {
        rig.press(cx, &["enter"]);
    }
    assert_eq!(rig.listing(cx), names(&ORIGINAL));
}

#[gpui_kit::test]
fn ctrl_z_while_the_done_item_shows_puts_the_names_back_without_asking(cx: &mut TestAppContext) {
    let journal = Journal::default();
    let rig = open(cx, files(), &journal);
    rig.rename_three(cx);
    assert_eq!(rig.listing(cx), names(&RENAMED));
    assert!(
        rig.done_detail(cx)
            .is_some_and(|text| text.contains("Z undo"))
    );
    rig.press(cx, &["ctrl-z"]);
    assert!(!rig.is_asking(cx));
    assert_eq!(rig.listing(cx), names(&ORIGINAL));
    assert_eq!(journal.state(), Some(State::Undone));
}

#[gpui_kit::test]
fn ctrl_z_after_the_done_item_is_gone_asks_first_and_then_puts_the_names_back(
    cx: &mut TestAppContext,
) {
    let journal = Journal::default();
    let rig = open(cx, files(), &journal);
    rig.rename_three(cx);
    rig.wait(cx, Duration::from_secs(9));
    assert_eq!(rig.done_detail(cx), None);
    rig.press(cx, &["ctrl-z"]);
    assert!(rig.is_asking(cx));
    assert_eq!(rig.listing(cx), names(&RENAMED));
    rig.press(cx, &["enter"]);
    assert!(!rig.is_asking(cx));
    assert_eq!(rig.listing(cx), names(&ORIGINAL));
}

#[gpui_kit::test]
fn an_item_changed_after_the_rename_is_skipped_and_counted(cx: &mut TestAppContext) {
    let journal = Journal::default();
    let fs = files();
    let rig = open(cx, fs.clone(), &journal);
    rig.rename_three(cx);
    let ops: &dyn ProviderOps = &*fs;
    ops.rename(&fs.location("home/xb.txt"), &fs.location("home/mine.txt"))
        .expect("rename");
    rig.press(cx, &["ctrl-z"]);
    assert_eq!(
        rig.listing(cx),
        names(&["docs", "a.txt", "c.txt", "d.txt", "mine.txt"])
    );
    assert_eq!(rig.skipped(cx), 1);
    assert!(
        rig.done_detail(cx)
            .is_some_and(|text| text.starts_with("1 item had changed"))
    );
}

#[gpui_kit::test]
fn nothing_to_undo_shows_on_the_status_line(cx: &mut TestAppContext) {
    let rig = open(cx, files(), &Journal::default());
    rig.press(cx, &["ctrl-z"]);
    assert_eq!(rig.notice(cx).as_deref(), Some("Nothing to undo"));
    assert!(!rig.is_asking(cx));
}

fn plan_of(fs: &FakeFs, pairs: &[(&str, &str)]) -> OperationPlan {
    let rows: Vec<Row> = pairs
        .iter()
        .map(|(old, new)| Row {
            parent: fs.location("home"),
            old: (*old).to_owned(),
            new: (*new).to_owned(),
            excluded: false,
        })
        .collect();
    let mut taken = Vec::new();
    let mut sink = |batch: EntryBatch| {
        taken.extend(batch.entries.into_iter().map(|e| ExistingName {
            parent: fs.location("home"),
            name: e.name.to_text(),
        }));
        ControlFlow::Continue(())
    };
    let _ = fs.list(&fs.location("home"), &mut sink, &CancelToken::new());
    plan_batch(&rows, &Existing::new(taken), Fold::Exact, 7)
        .plan
        .expect("a plan")
}

fn apply(fs: &FakeFs, plan: &OperationPlan) {
    let ops: &dyn ProviderOps = fs;
    for step in &plan.steps {
        if let Step::Rename { source, target } = step {
            ops.rename(source, target).expect("rename");
        }
    }
}

fn crashed_halfway(fs: &FakeFs, journal: &Journal) {
    let plan = plan_of(fs, &[("a.txt", "x1.txt"), ("b.txt", "x2.txt")]);
    apply(fs, &plan);
    let done = (0..u32::try_from(plan.steps.len()).expect("steps")).collect();
    journal.add(plan, (done, State::Pending), fs);
}

#[gpui_kit::test]
fn the_next_ctrl_z_asks_before_it_undoes_an_older_batch(cx: &mut TestAppContext) {
    let (fs, journal) = (files(), Journal::default());
    let older = plan_of(&fs, &[("d.txt", "x9.txt")]);
    apply(&fs, &older);
    journal.add(older, (vec![0], State::Done), &fs);
    let rig = open(cx, fs, &journal);
    rig.rename_three(cx);
    rig.press(cx, &["ctrl-z"]);
    assert_eq!(
        rig.listing(cx),
        names(&["docs", "a.txt", "b.txt", "c.txt", "x9.txt"])
    );
    rig.press(cx, &["ctrl-z"]);
    assert!(rig.is_asking(cx));
    assert_eq!(
        rig.listing(cx),
        names(&["docs", "a.txt", "b.txt", "c.txt", "x9.txt"])
    );
}

#[gpui_kit::test]
fn a_restore_pending_batch_gives_a_start_up_notice_and_ctrl_z_finishes_it(cx: &mut TestAppContext) {
    let (fs, journal) = (files(), Journal::default());
    crashed_halfway(&fs, &journal);
    let rig = open(cx, fs, &journal);
    assert!(
        rig.notice(cx)
            .is_some_and(|text| text.contains("did not finish"))
    );
    rig.press(cx, &["ctrl-z"]);
    assert!(rig.is_asking(cx));
    rig.press(cx, &["enter"]);
    assert_eq!(
        rig.listing(cx),
        names(&["docs", "a.txt", "b.txt", "c.txt", "d.txt"])
    );
    assert_eq!(journal.state(), Some(State::Undone));
}

#[gpui_kit::test]
fn a_hook_error_shows_on_the_status_line(cx: &mut TestAppContext) {
    let hooks = RenameHooks {
        log: None,
        last: Arc::new(|| Err("disk gone".to_owned())),
        ..Journal::default().hooks()
    };
    let rig = open_with(cx, files(), Operations::unlogged(), hooks);
    rig.press(cx, &["ctrl-z"]);
    assert!(
        rig.notice(cx)
            .is_some_and(|text| text.contains("disk gone"))
    );
}
