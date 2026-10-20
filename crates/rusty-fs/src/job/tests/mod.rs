use std::ops::ControlFlow;

use rusty_core::{
    ConflictPolicy, ExistingItem, FsError, FsErrorKind, ItemKind, OpKind, OperationPlan,
    PlanRequest, ScannedItem, Step, plan,
};

use super::*;
use crate::{
    Attributes, CancelToken, EntryBatch, EntryKind, FakeFs, FakeOp, Fault, FsEntry, Provider,
    ProviderOps,
};

mod cancel;
mod conflicts;
mod copy;
mod crash;
mod crashing;
mod faults;
mod journal;
mod kept_aside;
mod kept_aside_crash;
mod kept_aside_late;
mod kept_aside_race;
mod move_;
mod move_over;
mod move_over_crash;
mod move_over_race;
mod parallel;
mod parallel_pause;
mod parallel_props;
mod pause;
mod placed;
mod rename;
mod replace;
mod roll_back;
mod volume_proof;
mod workers;

const BIG: u64 = 10_000;
const MODES: [usize; 2] = [0, 4];

#[derive(Default)]
struct Log {
    entries: Vec<(u32, StepOutcome)>,
    fail_on: Option<u32>,
    held: Vec<HeldFile>,
    placed: Vec<PlacedFile>,
    unjournaled: bool,
    beside: Vec<rusty_core::Location>,
    stranded: Vec<HeldFile>,
}

fn log_error() -> FsError {
    FsError {
        os_code: Some(7),
        ..FsError::new(FsErrorKind::DiskFull)
    }
}

impl JobLog for Log {
    fn step(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        if self.fail_on == Some(seq) {
            return Err(log_error());
        }
        self.entries.push((seq, outcome));
        Ok(())
    }

    fn journal_held(&mut self, held: &HeldFile) -> Result<bool, FsError> {
        if self.unjournaled {
            return Ok(false);
        }
        self.held.push(held.clone());
        Ok(true)
    }

    fn journal_placed(&mut self, placed: &PlacedFile) -> Result<(), FsError> {
        self.placed.push(placed.clone());
        Ok(())
    }

    fn kept_beside(&mut self, _: &HeldFile, beside: &rusty_core::Location) {
        self.beside.push(beside.clone());
    }

    fn held_stranded(&mut self, held: &HeldFile) {
        self.stranded.push(held.clone());
    }
}

impl Log {
    fn done(&self) -> Vec<u32> {
        self.entries
            .iter()
            .filter(|(_, outcome)| *outcome == StepOutcome::Done)
            .map(|(seq, _)| *seq)
            .collect()
    }
}

#[derive(Default)]
struct Harness {
    log: Log,
    cancel: CancelToken,
    cancel_after_bytes: Option<u64>,
    progress: Vec<Progress>,
    workers: usize,
    keeps_held: bool,
    held: Vec<HeldFile>,
    placed: Vec<PlacedFile>,
}

fn stamps_of_done_copies(
    ops: &dyn ProviderOps,
    plan: &OperationPlan,
    done: &[u32],
) -> Vec<PlacedFile> {
    (0_u32..)
        .zip(&plan.steps)
        .filter(|(seq, _)| done.contains(seq))
        .filter_map(|(_, step)| match step {
            Step::CopyFile { target, .. } => ops.metadata(target).ok().map(|entry| PlacedFile {
                target: target.clone(),
                stamp: FileStamp {
                    size: entry.size,
                    modified_ms: entry.modified_ms,
                },
            }),
            _ => None,
        })
        .collect()
}

impl Harness {
    fn run(&mut self, ops: &dyn ProviderOps, plan: &OperationPlan, done: &[u32]) -> JobOutcome {
        let workers = self.workers;
        let outcome = self.with(ops, |ctx| match workers {
            0 => run_job(plan, done, ctx),
            n => super::parallel::run(plan, done, ctx, n),
        });
        if !self.keeps_held {
            let _ = release_held(&self.log.held, ops);
        }
        outcome
    }

    fn roll_back(
        &mut self,
        ops: &dyn ProviderOps,
        plan: &OperationPlan,
        done: &[u32],
    ) -> JobOutcome {
        let held = std::mem::take(&mut self.held);
        let placed = match self.placed.is_empty() {
            true => stamps_of_done_copies(ops, plan, done),
            false => self.placed.clone(),
        };
        let job = Journaled {
            plan,
            done,
            held: &held,
            placed: &placed,
            held_unreadable: false,
        };
        let outcome = self.with(ops, |ctx| roll_back(&job, ctx));
        self.held = held;
        outcome
    }

    fn with(
        &mut self,
        ops: &dyn ProviderOps,
        job: impl FnOnce(&mut JobContext<'_>) -> JobOutcome,
    ) -> JobOutcome {
        let cancel = &self.cancel;
        let limit = self.cancel_after_bytes;
        let seen = &mut self.progress;
        let mut on_progress = |progress: &Progress| {
            seen.push(progress.clone());
            if limit.is_some_and(|bytes| progress.bytes_done > bytes) {
                cancel.cancel();
            }
        };
        let mut ctx = JobContext {
            ops,
            log: &mut self.log,
            cancel,
            progress: &mut on_progress,
        };
        job(&mut ctx)
    }

    fn logged(&self) -> Vec<(u32, StepOutcome)> {
        self.log.entries.clone()
    }
}

fn run(ops: &dyn ProviderOps, plan: &OperationPlan) -> (JobOutcome, Harness) {
    run_in(0, ops, plan)
}

fn run_in(workers: usize, ops: &dyn ProviderOps, plan: &OperationPlan) -> (JobOutcome, Harness) {
    let mut harness = Harness {
        workers,
        ..Harness::default()
    };
    let outcome = harness.run(ops, plan, &[]);
    (outcome, harness)
}

fn copy_targets(plan: &OperationPlan) -> Vec<(u32, String, u64)> {
    (0_u32..)
        .zip(&plan.steps)
        .filter_map(|(seq, step)| match step {
            Step::CopyFile { target, size, .. } => Some((seq, target.path().to_text(), *size)),
            _ => None,
        })
        .collect()
}

fn placed_exactly_the_done_copies(fs: &FakeFs, plan: &OperationPlan, done: &[u32]) -> bool {
    copy_targets(plan).iter().all(|(seq, target, planned)| {
        let found = size(fs, target);
        if done.contains(seq) {
            found == Some(*planned)
        } else {
            found.is_none()
        }
    })
}

fn failure(outcome: &JobOutcome) -> Option<(u32, FsErrorKind)> {
    match outcome {
        JobOutcome::Failed { seq, error } => Some((*seq, error.kind)),
        JobOutcome::Done | JobOutcome::Cancelled => None,
    }
}

fn fixture(files: &[(&str, u64)]) -> FakeFs {
    let fs = FakeFs::new();
    fs.add_folder("src");
    fs.add_folder("dst");
    for (path, size) in files {
        let (folder, name) = path.rsplit_once('/').unwrap_or(("", path));
        fs.add_entry(
            folder,
            FsEntry {
                name: rusty_core::ProviderPath::from_text(name),
                kind: EntryKind::File,
                size: *size,
                modified_ms: Some(1_000),
                attributes: Attributes::default(),
            },
        );
    }
    fs
}

fn unplug(fs: &FakeFs, folder: &str) {
    fs.remove(folder);
    let seen = fs.calls("/", FakeOp::Metadata);
    for call in seen + 1..=seen + 64 {
        fs.fault(
            "/",
            FakeOp::Metadata,
            call,
            Fault::Fail(FsErrorKind::NotFound),
        );
    }
}

fn add_link(fs: &FakeFs, folder: &str, name: &str) {
    fs.add_entry(
        folder,
        FsEntry {
            name: rusty_core::ProviderPath::from_text(name),
            kind: EntryKind::Link,
            size: 0,
            modified_ms: None,
            attributes: Attributes::default(),
        },
    );
}

fn size(fs: &FakeFs, path: &str) -> Option<u64> {
    fs.metadata(&fs.location(path)).ok().map(|e| e.size)
}

fn exists(fs: &FakeFs, path: &str) -> bool {
    size(fs, path).is_some()
}

fn temp(path: &str) -> String {
    format!("{path}{TEMP_SUFFIX}")
}

fn children(fs: &FakeFs, path: &str) -> Vec<FsEntry> {
    let mut entries = Vec::new();
    let mut sink = |batch: EntryBatch| {
        entries.extend(batch.entries);
        ControlFlow::Continue(())
    };
    let _ = fs.list(&fs.location(path), &mut sink, &CancelToken::new());
    entries
}

fn join(folder: &str, name: &str) -> String {
    if folder.is_empty() {
        name.to_owned()
    } else {
        format!("{folder}/{name}")
    }
}

type Snapshot = Vec<(String, EntryKind, u64)>;

fn walk(fs: &FakeFs, root: &str, relative: &str, out: &mut Snapshot) {
    for entry in children(fs, &join(root, relative)) {
        let path = join(relative, &entry.name.to_text());
        out.push((path.clone(), entry.kind, entry.size));
        if entry.kind == EntryKind::Folder {
            walk(fs, root, &path, out);
        }
    }
}

fn tree_of(fs: &FakeFs, root: &str) -> Snapshot {
    let mut out = Vec::new();
    walk(fs, root, "", &mut out);
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn entry(path: &str, kind: EntryKind, size: u64) -> (String, EntryKind, u64) {
    (path.to_owned(), kind, size)
}

fn leftovers(fs: &FakeFs) -> Vec<String> {
    tree_of(fs, "")
        .into_iter()
        .map(|(path, _, _)| path)
        .filter(|path| path.ends_with(TEMP_SUFFIX))
        .collect()
}

fn item_kind(kind: EntryKind) -> ItemKind {
    match kind {
        EntryKind::File => ItemKind::File,
        EntryKind::Folder => ItemKind::Folder,
        EntryKind::Link => ItemKind::Link,
    }
}

fn scan(fs: &FakeFs, path: &str, depth: u32, out: &mut Vec<ScannedItem>) {
    let found = fs
        .metadata(&fs.location(path))
        .expect("scanned path exists");
    out.push(ScannedItem {
        location: fs.location(path),
        depth,
        kind: item_kind(found.kind),
        size: found.size,
        modified_ms: found.modified_ms,
    });
    if found.kind == EntryKind::Folder {
        for child in children(fs, path) {
            scan(fs, &join(path, &child.name.to_text()), depth + 1, out);
        }
    }
}

fn existing_under(fs: &FakeFs, target: &str) -> Vec<ExistingItem> {
    tree_of(fs, target)
        .into_iter()
        .map(|(path, kind, size)| ExistingItem {
            location: fs.location(&join(target, &path)),
            kind: item_kind(kind),
            size,
            modified_ms: None,
        })
        .collect()
}

fn request(fs: &FakeFs, kind: OpKind, sources: &[&str], target: &str) -> PlanRequest {
    let mut items = Vec::new();
    for source in sources {
        scan(fs, source, 0, &mut items);
    }
    PlanRequest {
        kind,
        items,
        target: Some(fs.location(target)),
        existing: existing_under(fs, target),
        policy: ConflictPolicy::Ask,
        same_volume: false,
        protected: Vec::new(),
    }
}

fn copy_plan(fs: &FakeFs, sources: &[&str], target: &str) -> OperationPlan {
    plan(&request(fs, OpKind::Copy, sources, target))
}

fn plan_with(fs: &FakeFs, sources: &[&str], policy: ConflictPolicy) -> OperationPlan {
    plan(&PlanRequest {
        policy,
        ..request(fs, OpKind::Copy, sources, "dst")
    })
}

fn all_seqs(plan: &OperationPlan) -> Vec<u32> {
    (0..plan.steps.len())
        .map(|seq| u32::try_from(seq).expect("small plan"))
        .collect()
}

fn three_files() -> FakeFs {
    fixture(&[
        ("src/one.txt", BIG),
        ("src/two.txt", 6_000),
        ("src/three.txt", 3_000),
    ])
}

fn copy_three(fs: &FakeFs) -> OperationPlan {
    copy_plan(fs, &["src/one.txt", "src/two.txt", "src/three.txt"], "dst")
}

#[test]
fn the_fixture_plan_copies_three_files_in_order() {
    let fs = three_files();
    let sources: Vec<_> = copy_three(&fs)
        .steps
        .iter()
        .filter_map(|step| match step {
            Step::CopyFile { source, .. } => Some(source.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        sources,
        ["src/one.txt", "src/two.txt", "src/three.txt"].map(|p| fs.location(p))
    );
}
