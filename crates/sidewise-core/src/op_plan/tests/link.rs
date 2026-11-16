use super::*;

fn make_link(source: &str, target: &str) -> Step {
    Step::MakeLink {
        target_of: loc(source),
        at: loc(target),
    }
}

#[test]
fn shortcut_names_follow_the_target_platform() {
    for (source, target, expected) in [
        (
            "/src/note.txt",
            r"D:\desk",
            r"D:\desk\note.txt - Shortcut.lnk",
        ),
        (
            "/src/note.txt",
            r"\\server\desk",
            r"\\server\desk\note.txt - Shortcut.lnk",
        ),
        (r"C:\src\note.txt", "/desk", "/desk/note.txt"),
    ] {
        let plan = plan(&request(
            OpKind::Link,
            vec![file(source, 0, 8)],
            Some(target),
        ));
        assert_eq!(plan.steps, [make_link(source, expected)]);
    }
}

#[test]
fn conflicts_number_links_without_overwriting_existing_or_planned_names() {
    for policy in [
        ConflictPolicy::Ask,
        ConflictPolicy::Skip,
        ConflictPolicy::Rename,
        ConflictPolicy::OverwriteAll,
        ConflictPolicy::OverwriteOlder,
    ] {
        let plan = plan(&PlanRequest {
            policy,
            existing: vec![
                existing(r"D:\desk\note.txt - Shortcut.lnk", ItemKind::Link, 0),
                existing(r"D:\desk\note.txt - Shortcut (2).lnk", ItemKind::Folder, 0),
            ],
            ..request(
                OpKind::Link,
                vec![file("/one/note.txt", 0, 8), file("/two/note.txt", 0, 9)],
                Some(r"D:\desk"),
            )
        });
        let destinations = [
            r"D:\desk\note.txt - Shortcut (3).lnk",
            r"D:\desk\note.txt - Shortcut (4).lnk",
        ];
        assert_eq!(
            plan.steps,
            [
                make_link("/one/note.txt", destinations[0]),
                make_link("/two/note.txt", destinations[1]),
            ]
        );
        assert_eq!(plan.conflicts.len(), 2);
        for (conflict, destination) in plan.conflicts.iter().zip(destinations) {
            assert_eq!(conflict.resolution, Resolution::Rename(loc(destination)));
        }
    }
}

#[test]
fn a_unix_link_in_its_source_folder_gets_a_number() {
    let plan = plan(&request(
        OpKind::Link,
        vec![file("/desk/note.txt", 0, 8)],
        Some("/desk"),
    ));
    assert_eq!(
        plan.steps,
        [make_link("/desk/note.txt", "/desk/note (2).txt")]
    );
}

#[test]
fn folders_and_links_each_make_one_link_and_totals_ignore_contents() {
    let plan = plan(&request(
        OpKind::Link,
        vec![
            folder("/src/docs", 0),
            file("/src/docs/large.bin", 1, 1 << 32),
            folder("/src/docs/nested", 1),
            file("/src/docs/nested/note.txt", 2, 4),
            link("/src/alias", 0),
        ],
        Some("/desk"),
    ));
    assert_eq!(
        plan.steps,
        [
            make_link("/src/docs", "/desk/docs"),
            make_link("/src/alias", "/desk/alias")
        ]
    );
    assert_eq!(
        plan.totals,
        Totals {
            files: 2,
            folders: 0,
            bytes: 0
        }
    );
    assert!(plan.risks.is_empty());
}

#[test]
fn an_old_journal_plan_json_still_parses() {
    let plan: OperationPlan =
        serde_json::from_str(include_str!("fixtures/old-copy-plan.json")).unwrap();
    assert_eq!(plan.kind, OpKind::Copy);
    assert_eq!(
        plan.steps,
        [
            make_folder("/dest/docs"),
            copy_file("/src/docs/note.txt", "/dest/docs/note.txt", 8)
        ]
    );
    assert_eq!(
        plan.totals,
        Totals {
            files: 1,
            folders: 1,
            bytes: 8
        }
    );
}
