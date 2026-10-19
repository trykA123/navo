use std::collections::HashMap;

use proptest::prelude::*;

use super::crashing::{Crashing, crashed_keeping};
use super::*;

#[derive(Debug, Clone)]
struct Spec {
    folders: Vec<usize>,
    files: Vec<(usize, u64)>,
    existing: Vec<(usize, u64)>,
    policy: ConflictPolicy,
    kind: OpKind,
    workers: usize,
}

fn folder_paths(parents: &[usize]) -> Vec<String> {
    let mut paths = vec!["top".to_owned()];
    for (index, pick) in parents.iter().enumerate() {
        let parent = paths[pick % paths.len()].clone();
        paths.push(format!("{parent}/d{index}"));
    }
    paths
}

fn file_paths(spec: &Spec) -> Vec<(String, u64)> {
    let folders = folder_paths(&spec.folders);
    spec.files
        .iter()
        .enumerate()
        .map(|(index, (pick, size))| {
            (
                format!("{}/f{index}.bin", folders[pick % folders.len()]),
                *size,
            )
        })
        .collect()
}

fn build(spec: &Spec) -> FakeFs {
    let fs = fixture(&[]);
    for folder in folder_paths(&spec.folders) {
        fs.add_folder(&format!("src/{folder}"));
    }
    let files = file_paths(spec);
    for (path, size) in &files {
        fs.add_file(&format!("src/{path}"), *size);
    }
    for (pick, size) in &spec.existing {
        if let Some((path, _)) = files.get(pick % files.len().max(1)) {
            fs.add_file(&format!("dst/{path}"), *size);
        }
    }
    fs
}

fn job_plan(fs: &FakeFs, spec: &Spec) -> OperationPlan {
    plan(&PlanRequest {
        policy: spec.policy.clone(),
        ..request(fs, spec.kind, &["src/top"], "dst")
    })
}

type Ending = (JobOutcome, Snapshot, Vec<(u32, StepOutcome)>, Vec<String>);

fn ending(spec: &Spec, workers: usize) -> Ending {
    let fs = build(spec);
    let plan = job_plan(&fs, spec);
    let mut harness = Harness {
        workers,
        ..Harness::default()
    };
    let outcome = harness.run(&fs, &plan, &[]);
    let mut logged = harness.logged();
    logged.sort_by_key(|(seq, _)| *seq);
    (outcome, tree_of(&fs, ""), logged, leftovers(&fs))
}

fn spec() -> impl Strategy<Value = Spec> {
    (
        prop::collection::vec(any::<usize>(), 0..8),
        prop::collection::vec((any::<usize>(), 0_u64..20_000), 0..24),
        prop::collection::vec((any::<usize>(), 0_u64..20_000), 0..6),
        prop_oneof![
            Just(ConflictPolicy::Skip),
            Just(ConflictPolicy::Rename),
            Just(ConflictPolicy::OverwriteAll),
        ],
        prop_oneof![Just(OpKind::Copy), Just(OpKind::Move)],
        1_usize..=8,
    )
        .prop_map(|(folders, files, existing, policy, kind, workers)| Spec {
            folders,
            files,
            existing,
            policy,
            kind,
            workers,
        })
}

#[derive(Debug, Clone)]
enum Interruption {
    Fail(usize),
    Cancel(u64),
    Crash(usize),
}

fn interruption() -> impl Strategy<Value = Interruption> {
    prop_oneof![
        any::<usize>().prop_map(Interruption::Fail),
        (0_u64..200_000).prop_map(Interruption::Cancel),
        any::<usize>().prop_map(Interruption::Crash),
    ]
}

struct Interrupted {
    fs: FakeFs,
    plan: OperationPlan,
    before: HashMap<String, u64>,
    failed: Option<(u32, FsErrorKind)>,
    done: Vec<u32>,
    held: Vec<HeldFile>,
}

fn source_of(spec: &Spec, pick: usize) -> Option<String> {
    let files = file_paths(spec);
    let (path, _) = files.get(pick % files.len().max(1))?;
    Some(format!("src/{path}"))
}

fn interrupted(spec: &Spec, interruption: &Interruption, workers: usize) -> Interrupted {
    let fs = build(spec);
    let plan = job_plan(&fs, spec);
    let before = tree_of(&fs, "")
        .into_iter()
        .map(|(path, _, size)| (path, size))
        .collect();
    let mut harness = Harness {
        workers,
        ..Harness::default()
    };
    let mut failed = None;
    let mut held = Vec::new();
    let done = match interruption {
        Interruption::Crash(pick) => {
            let path = source_of(spec, *pick).unwrap_or_default();
            let crashing = Crashing::new(&fs, &path, FakeOp::Copy);
            let (done, kept) = crashed_keeping(&crashing, &plan, workers);
            held = kept;
            let _ = clean_up(&plan, &done, &fs);
            done
        }
        Interruption::Fail(pick) => {
            if let Some(path) = source_of(spec, *pick) {
                fs.fault(&path, FakeOp::Copy, 1, Fault::Fail(FsErrorKind::DiskFull));
            }
            failed = failure(&harness.run(&fs, &plan, &[]));
            harness.log.done()
        }
        Interruption::Cancel(bytes) => {
            harness.cancel_after_bytes = Some(*bytes);
            let _ = harness.run(&fs, &plan, &[]);
            harness.log.done()
        }
    };
    Interrupted {
        fs,
        plan,
        before,
        failed,
        done,
        held,
    }
}

fn holds_only_done_copies(run: &Interrupted) -> bool {
    (0_u32..).zip(&run.plan.steps).all(|(seq, step)| {
        let Step::CopyFile {
            target,
            size: planned,
            ..
        } = step
        else {
            return true;
        };
        let path = target.path().to_text();
        let found = size(&run.fs, &path);
        let before = run.before.get(&path).copied();
        if run.done.contains(&seq) {
            found == Some(*planned)
        } else {
            found == before || (found.is_none() && overwrites(&run.plan, target))
        }
    })
}

fn overwrites(plan: &OperationPlan, target: &rusty_core::Location) -> bool {
    plan.conflicts
        .iter()
        .any(|c| c.resolution == rusty_core::Resolution::Overwrite && c.target.same_place(target))
}

fn resumed(run: &Interrupted, workers: usize) -> (JobOutcome, Snapshot, Vec<String>) {
    let mut harness = Harness {
        workers,
        ..Harness::default()
    };
    let outcome = harness.run(&run.fs, &run.plan, &run.done);
    let _ = release_held(&run.held, &run.fs);
    (outcome, tree_of(&run.fs, ""), leftovers(&run.fs))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    #[test]
    fn a_parallel_job_ends_like_the_standard_job(spec in spec()) {
        let standard = ending(&spec, 0);
        prop_assert_eq!(&standard.0, &JobOutcome::Done);
        prop_assert_eq!(ending(&spec, spec.workers), standard);
    }

    #[test]
    fn an_interrupted_parallel_job_resumes_to_the_standard_result(
        spec in spec(),
        interruption in interruption(),
    ) {
        let clean = ending(&spec, 0).1;
        let standard = interrupted(&spec, &interruption, 0);
        let parallel = interrupted(&spec, &interruption, spec.workers);
        prop_assert_eq!(parallel.failed, standard.failed);
        for (run, workers) in [(&standard, 0), (&parallel, spec.workers)] {
            prop_assert_eq!(leftovers(&run.fs), Vec::<String>::new());
            prop_assert!(holds_only_done_copies(run), "{} workers", workers);
            prop_assert_eq!(
                resumed(run, workers),
                (JobOutcome::Done, clean.clone(), Vec::new())
            );
        }
    }
}
