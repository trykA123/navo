use proptest::prelude::*;

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

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    #[test]
    fn a_parallel_job_ends_like_the_standard_job(spec in spec()) {
        let standard = ending(&spec, 0);
        prop_assert_eq!(&standard.0, &JobOutcome::Done);
        prop_assert_eq!(ending(&spec, spec.workers), standard);
    }
}
