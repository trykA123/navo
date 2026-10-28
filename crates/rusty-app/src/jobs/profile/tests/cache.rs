use super::*;

fn at(path: &Path) -> Location {
    crate::startup::path_location(path).unwrap()
}

fn mixed(root: &Path) -> OperationPlan {
    let mut steps = vec![];
    for folder in ["one", "two", "three"] {
        let parent = root.join("files").join(folder);
        std::fs::create_dir_all(&parent).unwrap();
        for file in 0..3 {
            steps.push(Step::CopyFile {
                source: at(&root.join("source")),
                target: at(&parent.join(format!("file-{file}"))),
                size: 0,
            });
        }
    }
    OperationPlan {
        kind: OpKind::Copy,
        steps,
        totals: rusty_core::Totals {
            files: 9,
            folders: 0,
            bytes: 0,
        },
        conflicts: vec![],
        risks: vec![],
    }
}

fn compare(plan: &OperationPlan, dirs: &StoreDirs, experimental: bool) -> Result<(), String> {
    let uncached = plan
        .steps
        .iter()
        .flat_map(mutated)
        .map(rusty_fs::mutation_location)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.message());
    assert_eq!(mutated_paths(plan), uncached);
    let expected = uncached.and_then(|paths| guard_paths(&paths, dirs, experimental));
    let cached = guard(plan, dirs, experimental);
    assert_eq!(cached, expected);
    cached
}

fn compare_refusals(plan: &OperationPlan, dirs: &StoreDirs, root: &Path) {
    for experimental in [false, true] {
        for path in [
            dirs.config.join("new"),
            dirs.data.join("new"),
            root.join("files/one/.navo-retained-ab/payload"),
            root.join("files/two/.navo-held-1-2"),
            root.join("files/three/.navo-mutations-runtime/new"),
        ] {
            let mut plan = plan.clone();
            plan.steps.push(Step::RemoveFile {
                location: at(&path),
            });
            assert!(compare(&plan, dirs, experimental).is_err());
        }
    }
}

#[test]
fn cached_and_uncached_mixed_plans_preserve_profile_and_reserved_refusals() {
    let (root, mut dirs, _journal) = setup();
    let plan = mixed(root.path());
    for experimental in [false, true] {
        compare(&plan, &dirs, experimental).unwrap();
    }
    compare_refusals(&plan, &dirs, root.path());
    dirs.config = root.path().join("missing/config");
    compare(&plan, &dirs, false).unwrap();
    assert!(compare(&plan, &dirs, true).is_err());
    compare_refusals(&plan, &dirs, root.path());
}
