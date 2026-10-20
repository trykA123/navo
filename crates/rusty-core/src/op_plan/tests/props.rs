use proptest::prelude::*;

use super::*;
use crate::ProviderPath;

const MAX_DEPTH: u32 = 4;

type Node = (u8, u32, u64);

fn node() -> impl Strategy<Value = Node> {
    (0..3_u8, 0..=MAX_DEPTH, 0..5_000_u64)
}

fn op_kind() -> impl Strategy<Value = OpKind> {
    prop_oneof![
        Just(OpKind::Copy),
        Just(OpKind::Move),
        Just(OpKind::Recycle),
        Just(OpKind::Delete),
    ]
}

fn tree(nodes: &[Node]) -> Vec<ScannedItem> {
    let root = loc(r"C:\src");
    let mut folders: Vec<Location> = Vec::new();
    let mut previous: Option<(u32, ItemKind)> = None;
    let mut items = Vec::new();
    for (index, &(kind, wanted, size)) in nodes.iter().enumerate() {
        let deepest = match &previous {
            Some((depth, ItemKind::Folder)) => depth + 1,
            Some((depth, _)) => *depth,
            None => 0,
        };
        let depth = wanted.min(deepest);
        folders.truncate(depth as usize);
        let parent = folders.last().unwrap_or(&root);
        let (kind, name, size) = match kind {
            0 => (ItemKind::File, format!("n{index}.txt"), size),
            1 => (ItemKind::Folder, format!("n{index}"), 0),
            _ => (ItemKind::Link, format!("n{index}.lnk"), 0),
        };
        let location = parent.join(&ProviderPath::from_text(&name)).unwrap();
        if kind == ItemKind::Folder {
            folders.push(location.clone());
        }
        previous = Some((depth, kind.clone()));
        items.push(ScannedItem {
            location,
            depth,
            kind,
            size,
            modified_ms: Some(1_000),
        });
    }
    items
}

fn random_plan() -> impl Strategy<Value = (Vec<ScannedItem>, OperationPlan)> {
    (
        prop::collection::vec(node(), 1..=40),
        op_kind(),
        any::<bool>(),
    )
        .prop_map(|(nodes, kind, same_volume)| {
            let items = tree(&nodes);
            let target = matches!(kind, OpKind::Copy | OpKind::Move).then_some(r"D:\dst");
            let request = PlanRequest {
                same_volume,
                ..request(kind, items.clone(), target)
            };
            (items, plan(&request))
        })
}

fn expected_totals(items: &[ScannedItem], plan: &OperationPlan) -> Totals {
    let count = |wanted: fn(&Step) -> bool| plan.steps.iter().filter(|s| wanted(s)).count() as u64;
    let copied_bytes = plan
        .steps
        .iter()
        .map(|s| match s {
            Step::CopyFile { size, .. } => *size,
            _ => 0,
        })
        .sum();
    let moved = moved_items(items, plan);
    let scanned =
        |wanted: fn(&ItemKind) -> bool| moved.iter().filter(|i| wanted(&i.kind)).count() as u64;
    let scanned_bytes = moved
        .iter()
        .filter(|i| i.kind == ItemKind::File)
        .map(|i| i.size)
        .sum();
    match (&plan.kind, renamed_or_recycled(plan)) {
        (OpKind::Recycle, _) | (OpKind::Move, true) => Totals {
            files: scanned(|k| *k == ItemKind::File),
            folders: scanned(|k| *k == ItemKind::Folder),
            bytes: scanned_bytes,
        },
        (OpKind::Copy | OpKind::Move, _) => Totals {
            files: count(|s| matches!(s, Step::CopyFile { .. })),
            folders: count(|s| matches!(s, Step::MakeFolder { .. })),
            bytes: copied_bytes,
        },
        (OpKind::Rename, _) => Totals {
            files: 0,
            folders: 0,
            bytes: 0,
        },
        (OpKind::Delete, _) => Totals {
            files: count(|s| matches!(s, Step::RemoveFile { .. })),
            folders: count(|s| matches!(s, Step::RemoveFolder { .. })),
            bytes: items
                .iter()
                .filter(|i| i.kind == ItemKind::File)
                .map(|i| i.size)
                .sum(),
        },
    }
}

fn renamed_or_recycled(plan: &OperationPlan) -> bool {
    plan.steps
        .iter()
        .any(|s| matches!(s, Step::Rename { .. } | Step::Recycle { .. }))
}

fn moved_items<'a>(items: &'a [ScannedItem], plan: &OperationPlan) -> Vec<&'a ScannedItem> {
    let tops: Vec<&Location> = plan
        .steps
        .iter()
        .filter_map(|s| match s {
            Step::Rename { source, .. } => Some(source),
            Step::Recycle { location } => Some(location),
            _ => None,
        })
        .collect();
    let mut top: Option<&Location> = None;
    items
        .iter()
        .filter(|item| {
            if item.depth == 0 {
                top = Some(&item.location);
            }
            top.is_some_and(|t| tops.iter().any(|planned| planned.same_place(t)))
        })
        .collect()
}

fn locations(step: &Step) -> Vec<&Location> {
    match step {
        Step::CopyFile { source, target, .. } | Step::Rename { source, target } => {
            vec![source, target]
        }
        Step::MakeFolder { target } => vec![target],
        Step::Recycle { location }
        | Step::RemoveFile { location }
        | Step::RemoveLink { location }
        | Step::RemoveFolder { location } => vec![location],
    }
}

fn inside(location: &Location, folder: &Location) -> bool {
    std::iter::successors(location.parent(), Location::parent).any(|a| a.same_place(folder))
}

fn touches_inside(step: &Step, folder: &Location) -> bool {
    locations(step).into_iter().any(|l| inside(l, folder))
}

proptest! {
    #[test]
    fn totals_equal_the_counts_over_the_steps((items, plan) in random_plan()) {
        prop_assert_eq!(&plan.totals, &expected_totals(&items, &plan));
    }

    #[test]
    fn a_folder_is_removed_after_every_step_inside_it((_items, plan) in random_plan()) {
        for (index, step) in plan.steps.iter().enumerate() {
            if let Step::RemoveFolder { location } = step {
                let later = plan.steps[index..].iter().any(|s| touches_inside(s, location));
                prop_assert!(!later, "{location:?} is removed before a step inside it");
            }
        }
    }

    #[test]
    fn a_folder_is_made_before_every_step_inside_it((_items, plan) in random_plan()) {
        for (index, step) in plan.steps.iter().enumerate() {
            if let Step::MakeFolder { target } = step {
                let earlier = plan.steps[..index].iter().any(|s| touches_inside(s, target));
                prop_assert!(!earlier, "{target:?} is made after a step inside it");
            }
        }
    }

    #[test]
    fn the_plan_round_trips_through_json((_items, plan) in random_plan()) {
        let text = serde_json::to_string(&plan).unwrap();
        let back: OperationPlan = serde_json::from_str(&text).unwrap();
        prop_assert_eq!(back, plan);
    }
}
