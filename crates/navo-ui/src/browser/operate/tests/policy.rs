//! Conflict policy in the request: the planning job plans with the policy the request carries.
//! The conflict sheet (CS1) sets it; until then every request asks, which skips existing targets.

use navo_core::{AppState, ConflictPolicy, OpKind, ProviderPath, Resolution, Step};
use navo_fs::{Attributes, CancelToken, EntryKind, FakeFs, FsEntry};

use super::super::OpRequest;
use super::super::work::plan_for;

/// Adds a file of 3 bytes named `name` in `folder`, last modified at `modified_ms`.
fn stamped(fs: &FakeFs, folder: &str, name: &str, modified_ms: i64) {
    fs.add_entry(
        folder,
        FsEntry {
            name: ProviderPath::from_text(name),
            kind: EntryKind::File,
            size: 3,
            modified_ms: Some(modified_ms),
            attributes: Attributes::default(),
        },
    );
}

/// Plans a copy of `home/new.txt` and `home/old.txt` into `dest`, where both names exist.
fn copy_plan(policy: ConflictPolicy) -> (FakeFs, navo_core::OperationPlan) {
    let fs = FakeFs::new();
    fs.add_folder("home");
    fs.add_folder("dest");
    stamped(&fs, "home", "new.txt", 2_000);
    stamped(&fs, "dest", "new.txt", 1_000);
    stamped(&fs, "home", "old.txt", 1_000);
    stamped(&fs, "dest", "old.txt", 2_000);
    let state = AppState::new(fs.location("home"));
    let request = OpRequest {
        id: 1,
        kind: OpKind::Copy,
        sources: vec![fs.location("home/new.txt"), fs.location("home/old.txt")],
        target: Some(fs.location("dest")),
        same_volume: false,
        protected: Vec::new(),
        source_tab: state.active_tab().id(),
        asks: true,
        structure: None,
        stream: None,
        policy,
    };
    let plan = plan_for(&fs, &request, &CancelToken::new()).unwrap();
    (fs, plan)
}

#[test]
fn overwrite_older_plans_an_overwrite_only_for_the_newer_source() {
    let (fs, plan) = copy_plan(ConflictPolicy::OverwriteOlder);
    let resolution = |name: &str| {
        plan.conflicts
            .iter()
            .find(|c| c.source == fs.location(&format!("home/{name}")))
            .map(|c| c.resolution.clone())
    };
    assert_eq!(resolution("new.txt"), Some(Resolution::Overwrite));
    assert_eq!(resolution("old.txt"), Some(Resolution::Skip));
    let copied: Vec<_> = plan
        .steps
        .iter()
        .filter_map(|step| match step {
            Step::CopyFile { source, .. } => Some(source.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(copied, [fs.location("home/new.txt")]);
}

#[test]
fn an_asking_request_plans_no_step_for_either_existing_target() {
    let (_, plan) = copy_plan(ConflictPolicy::Ask);
    assert!(
        plan.conflicts
            .iter()
            .all(|c| c.resolution == Resolution::Ask)
    );
    assert_eq!(plan.conflicts.len(), 2);
    assert!(
        !plan
            .steps
            .iter()
            .any(|s| matches!(s, Step::CopyFile { .. }))
    );
}
