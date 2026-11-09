//! Root selection for the job-wide Git pause: component-wise sort, then one covered-root pass.

use navo_core::{Location, OpKind, OperationPlan, ProviderPath, Step, Totals};

use super::super::git_pause::pause_roots;

/// A delete plan over `steps` with empty totals.
fn delete(steps: Vec<Step>) -> OperationPlan {
    OperationPlan {
        kind: OpKind::Delete,
        steps,
        totals: Totals {
            files: 0,
            folders: 0,
            bytes: 0,
        },
        conflicts: Vec::new(),
        risks: Vec::new(),
    }
}

/// A remove-folder step at `path`.
fn folder(path: &str) -> Step {
    Step::RemoveFolder {
        location: Location::new(path).unwrap(),
    }
}

#[test]
fn a_deep_tree_collapses_to_its_one_root() {
    let root = Location::new("root").unwrap();
    let mut steps = vec![Step::RemoveFolder {
        location: root.clone(),
    }];
    for index in 0..20_000 {
        steps.push(Step::RemoveFile {
            location: root
                .join(&ProviderPath::from_text(&format!("f{index}")))
                .unwrap(),
        });
    }
    assert_eq!(pause_roots(&delete(steps)), vec![root]);
}

#[test]
fn sibling_roots_sharing_a_prefix_stay_separate() {
    let roots = pause_roots(&delete(vec![folder("repo"), folder("repo2")]));
    assert_eq!(
        roots,
        vec![
            Location::new("repo").unwrap(),
            Location::new("repo2").unwrap()
        ]
    );
}

#[test]
fn a_nested_chosen_item_keeps_only_the_outer_root() {
    let roots = pause_roots(&delete(vec![folder("a/b"), folder("a/b/c")]));
    assert_eq!(roots, vec![Location::new("a/b").unwrap()]);
}
