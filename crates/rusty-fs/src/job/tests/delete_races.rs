use std::sync::{Condvar, Mutex};
use std::thread::scope;
use std::time::Duration;

use rusty_core::{Location, ProviderId};

use super::super::marks::WHOLE_JOB;
use super::parallel_delete::{delete_all, location_of, removed, tree};
use super::*;
use crate::{Capabilities, ListSink};

const WAIT: Duration = Duration::from_secs(5);

struct BeforeRemove<'a> {
    fs: &'a FakeFs,
    hook: &'a (dyn Fn(&Location) + Sync),
}

impl Provider for BeforeRemove<'_> {
    fn id(&self) -> ProviderId {
        self.fs.id()
    }

    fn capabilities(&self) -> Capabilities {
        self.fs.capabilities()
    }

    fn list(
        &self,
        folder: &Location,
        sink: &mut dyn ListSink,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        self.fs.list(folder, sink, cancel)
    }

    fn metadata(&self, location: &Location) -> Result<FsEntry, FsError> {
        self.fs.metadata(location)
    }
}

impl ProviderOps for BeforeRemove<'_> {
    fn create_folder(&self, location: &Location) -> Result<(), FsError> {
        self.fs.create_folder(location)
    }

    fn create_file(&self, location: &Location) -> Result<(), FsError> {
        self.fs.create_file(location)
    }

    fn copy_file(
        &self,
        source: &Location,
        target: &Location,
        progress: &mut dyn FnMut(u64),
        cancel: &CancelToken,
    ) -> Result<u64, FsError> {
        self.fs.copy_file(source, target, progress, cancel)
    }

    fn rename(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.fs.rename(source, target)
    }

    fn replace(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.fs.replace(source, target)
    }

    fn remove_file(&self, location: &Location) -> Result<(), FsError> {
        (self.hook)(location);
        self.fs.remove_file(location)
    }

    fn make_link(&self, target_of: &Location, at: &Location) -> Result<(), FsError> {
        self.fs.make_link(target_of, at)
    }

    fn remove_link(&self, location: &Location) -> Result<(), FsError> {
        (self.hook)(location);
        self.fs.remove_link(location)
    }

    fn remove_empty_folder(&self, location: &Location) -> Result<(), FsError> {
        (self.hook)(location);
        self.fs.remove_empty_folder(location)
    }

    fn recycle(&self, location: &Location) -> Result<(), FsError> {
        self.fs.recycle(location)
    }
}

#[derive(Default)]
struct Flag {
    set: Mutex<bool>,
    wake: Condvar,
}

impl Flag {
    fn raise(&self) {
        *self.set.lock().expect("flag") = true;
        self.wake.notify_all();
    }

    fn wait(&self) -> bool {
        let set = self.set.lock().expect("flag");
        let set = self.wake.wait_timeout_while(set, WAIT, |set| !*set);
        set.is_ok_and(|(set, _)| *set)
    }
}

#[derive(Default)]
struct Marked {
    seqs: Mutex<Vec<u32>>,
    wake: Condvar,
}

impl Marked {
    fn wait_until(&self, holds: impl Fn(&[u32]) -> bool) -> bool {
        let seqs = self.seqs.lock().expect("marked");
        let seqs = self
            .wake
            .wait_timeout_while(seqs, WAIT, |seqs| !holds(seqs));
        seqs.is_ok_and(|(seqs, _)| holds(&seqs))
    }
}

struct Observed<'a> {
    log: Log,
    marked: &'a Marked,
}

impl Observed<'_> {
    fn note(&self, marks: &[(u32, StepOutcome)]) {
        let mut seqs = self.marked.seqs.lock().expect("marked");
        seqs.extend(
            marks
                .iter()
                .filter(|(_, outcome)| *outcome != StepOutcome::Failed)
                .map(|(seq, _)| *seq),
        );
        seqs.sort_unstable();
        self.marked.wake.notify_all();
    }
}

impl JobLog for Observed<'_> {
    fn step(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        self.log.step(seq, outcome)?;
        self.note(&[(seq, outcome)]);
        Ok(())
    }

    fn steps(&mut self, marks: &[(u32, StepOutcome)]) -> Result<(), FsError> {
        self.log.steps(marks)?;
        self.note(marks);
        Ok(())
    }

    fn undone(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        self.log.undone(seq, outcome)
    }

    fn journal_held(&mut self, held: &HeldFile) -> Result<bool, FsError> {
        self.log.journal_held(held)
    }
}

fn nested_tree() -> FakeFs {
    let fs = fixture(&[]);
    for folder in 0..4 {
        for file in 0..30 {
            fs.add_file(&format!("top/src/e{folder}/f{file}.txt"), 1);
        }
    }
    fs
}

#[test]
fn a_delete_whose_root_parent_is_renamed_mid_job_fails_and_skips_nothing() {
    for workers in MODES {
        let fs = nested_tree();
        let plan = plan(&PlanRequest {
            target: None,
            existing: Vec::new(),
            ..request(&fs, OpKind::Delete, &["top/src"], "dst")
        });
        let calls = Mutex::new(0_usize);
        let rename_parent = |_: &Location| {
            let mut calls = calls.lock().unwrap();
            *calls += 1;
            if *calls == 6 {
                fs.rename_entry("top", "moved");
            }
        };
        let ops = BeforeRemove {
            fs: &fs,
            hook: &rename_parent,
        };
        let (outcome, harness) = run_in(workers, &ops, &plan);
        let skipped = harness
            .log
            .entries
            .iter()
            .filter(|(_, outcome)| *outcome == StepOutcome::Skipped)
            .count();
        assert_eq!(
            (failure(&outcome).map(|(_, kind)| kind), skipped),
            (Some(FsErrorKind::NotFound), 0),
            "{workers}"
        );
        assert!(tree_of(&fs, "moved").len() > 100, "{workers}");
    }
}

fn resumes_after_losing(gone: &str) {
    for workers in MODES {
        let fs = tree(12, 40);
        let plan = delete_all(&fs);
        fs.remove(gone);
        let missing: Vec<u32> = removed(&fs, &plan);
        let (outcome, harness) = run_in(workers, &fs, &plan);
        let mut skipped: Vec<u32> = harness
            .log
            .entries
            .iter()
            .filter(|(_, outcome)| *outcome == StepOutcome::Skipped)
            .map(|(seq, _)| *seq)
            .collect();
        skipped.sort_unstable();
        assert_eq!(
            (outcome, exists(&fs, "src"), skipped),
            (JobOutcome::Done, false, missing),
            "{gone} {workers}"
        );
    }
}

#[test]
fn a_delete_resumes_when_a_folder_and_its_children_lost_their_marks_together() {
    resumes_after_losing("src/d0");
}

#[test]
fn a_delete_resumes_when_every_mark_was_lost_after_the_root_went() {
    resumes_after_losing("src");
}

fn run_observed(
    ops: &dyn ProviderOps,
    plan: &OperationPlan,
    (cancel, workers): (&CancelToken, usize),
    (marked, paused): (&Marked, &Flag),
) -> JobOutcome {
    let mut log = Observed {
        log: Log::default(),
        marked,
    };
    let mut report = |progress: &Progress| {
        if progress.paused {
            paused.raise();
        }
    };
    run_job_parallel(
        plan,
        &[],
        &mut JobContext {
            ops,
            log: &mut log,
            cancel,
            progress: &mut report,
        },
        workers,
    )
}

#[test]
fn a_paused_delete_has_journalled_every_finished_file() {
    for workers in [2, 4] {
        let fs = tree(10, 100);
        let plan = delete_all(&fs);
        let middle = fs.location("src/d1/e4/f50.txt");
        let (cancel, marked, paused) = (CancelToken::new(), Marked::default(), Flag::default());
        let pause_inside_a_remove = |location: &Location| {
            if *location == middle {
                cancel.pause();
                paused.wait();
            }
        };
        let ops = BeforeRemove {
            fs: &fs,
            hook: &pause_inside_a_remove,
        };
        let (outcome, journalled) = scope(|s| {
            let job = s.spawn(|| run_observed(&ops, &plan, (&cancel, workers), (&marked, &paused)));
            let at_rest = paused.wait()
                && marked.wait_until(|seqs| {
                    !exists(&fs, "src/d1/e4/f50.txt") && seqs == removed(&fs, &plan).as_slice()
                });
            cancel.resume();
            (job.join().expect("the job ends"), at_rest)
        });
        assert_eq!((outcome, journalled), (JobOutcome::Done, true), "{workers}");
    }
}

#[test]
fn a_delete_waiting_on_a_slow_remove_journals_the_finished_ones_within_a_moment() {
    for workers in [2, 4] {
        let fs = tree(10, 100);
        let plan = delete_all(&fs);
        let first_folder = plan
            .steps
            .iter()
            .position(|step| matches!(step, Step::RemoveFolder { .. }))
            .expect("a folder");
        let slow = location_of(&plan.steps[first_folder - 1]).clone();
        let (cancel, marked, released) = (CancelToken::new(), Marked::default(), Flag::default());
        let hold_the_last_file = |location: &Location| {
            if *location == slow {
                released.wait();
            }
        };
        let ops = BeforeRemove {
            fs: &fs,
            hook: &hold_the_last_file,
        };
        let (outcome, journalled) = scope(|s| {
            let job = s.spawn(|| {
                run_observed(&ops, &plan, (&cancel, workers), (&marked, &Flag::default()))
            });
            let caught_up = marked.wait_until(|seqs| {
                seqs.len() + 1 >= first_folder && seqs == removed(&fs, &plan).as_slice()
            });
            released.raise();
            (job.join().expect("the job ends"), caught_up)
        });
        assert_eq!((outcome, journalled), (JobOutcome::Done, true), "{workers}");
    }
}

#[test]
fn a_failed_journal_batch_fails_the_whole_job_without_blaming_a_file() {
    for workers in MODES {
        let fs = tree(4, 30);
        let plan = delete_all(&fs);
        let mut harness = Harness {
            workers,
            log: Log {
                fail_on: Some(7),
                ..Log::default()
            },
            ..Harness::default()
        };
        let outcome = harness.run(&fs, &plan, &[]);
        let blamed = match outcome {
            JobOutcome::Failed { seq, error } => Some((seq, error.kind, error.location)),
            JobOutcome::Done | JobOutcome::Cancelled => None,
        };
        assert_eq!(
            blamed,
            Some((WHOLE_JOB, FsErrorKind::JournalWriteFailed, None)),
            "{workers}"
        );
    }
}
