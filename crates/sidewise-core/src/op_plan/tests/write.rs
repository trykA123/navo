use super::*;

fn stamp(size: u64, modified_ms: i64) -> FileStamp {
    FileStamp {
        size,
        modified_ms: Some(modified_ms),
        hash: [0xab; 32],
    }
}

#[test]
fn a_write_plan_has_one_write_step_and_the_old_stamp() {
    let plan = write_plan(
        &loc("/dest/note.txt"),
        &loc("/dest/.note.txt.tmp"),
        stamp(7, 1_000),
        11,
    );
    assert_eq!(plan.kind, OpKind::Save);
    assert_eq!(
        plan.steps,
        [Step::WriteFile {
            target: loc("/dest/note.txt"),
            staged: loc("/dest/.note.txt.tmp"),
            staged_len: 11,
            expect: stamp(7, 1_000),
        }]
    );
    assert_eq!(
        plan.totals,
        Totals {
            files: 1,
            folders: 0,
            bytes: 11
        }
    );
    assert_eq!(plan.risks, [Risk::Overwrite(loc("/dest/note.txt"))]);
    assert!(plan.conflicts.is_empty());
}

#[test]
fn the_hash_serializes_as_lowercase_hex_and_round_trips() {
    let mut hash = [0_u8; 32];
    hash[0] = 1;
    hash[31] = 0xff;
    let expect = FileStamp {
        size: 7,
        modified_ms: Some(1_000),
        hash,
    };
    let plan = write_plan(
        &loc("/dest/note.txt"),
        &loc("/dest/.note.txt.tmp"),
        expect,
        11,
    );
    let json = serde_json::to_value(&plan).unwrap();
    let text = json["steps"][0]["WriteFile"]["expect"]["hash"]
        .as_str()
        .unwrap();
    assert_eq!(text, format!("01{}ff", "00".repeat(30)));
    assert_eq!(serde_json::from_value::<OperationPlan>(json).unwrap(), plan);
}

#[test]
fn a_write_plan_without_a_modification_time_round_trips() {
    let plan = write_plan(
        &loc("/dest/note.txt"),
        &loc("/dest/.note.txt.tmp"),
        FileStamp {
            size: 0,
            modified_ms: None,
            hash: [0_u8; 32],
        },
        0,
    );
    let json = serde_json::to_value(&plan).unwrap();
    assert_eq!(
        json["steps"][0]["WriteFile"]["expect"]["modified_ms"],
        serde_json::Value::Null
    );
    assert_eq!(serde_json::from_value::<OperationPlan>(json).unwrap(), plan);
}

#[test]
fn an_old_plan_without_a_write_step_still_parses() {
    let plan: OperationPlan =
        serde_json::from_str(include_str!("fixtures/old-copy-plan.json")).unwrap();
    assert_eq!(plan.kind, OpKind::Copy);
    assert!(
        plan.steps
            .iter()
            .all(|step| !matches!(step, Step::WriteFile { .. }))
    );
}

fn write_target(target: &str, staged: &str, staged_len: u64) -> WriteTarget {
    WriteTarget {
        target: loc(target),
        staged: loc(staged),
        expect: stamp(7, 1_000),
        staged_len,
    }
}

#[test]
fn a_write_many_plan_has_one_step_and_one_overwrite_risk_per_target() {
    let plan = write_many_plan(&[
        write_target("/dest/a.txt", "/dest/.a.txt.tmp", 11),
        write_target("/dest/b.txt", "/dest/.b.txt.tmp", 13),
    ])
    .unwrap();
    assert_eq!(plan.kind, OpKind::Save);
    assert_eq!(
        plan.steps,
        [
            Step::WriteFile {
                target: loc("/dest/a.txt"),
                staged: loc("/dest/.a.txt.tmp"),
                staged_len: 11,
                expect: stamp(7, 1_000),
            },
            Step::WriteFile {
                target: loc("/dest/b.txt"),
                staged: loc("/dest/.b.txt.tmp"),
                staged_len: 13,
                expect: stamp(7, 1_000),
            },
        ]
    );
    assert_eq!(
        plan.totals,
        Totals {
            files: 2,
            folders: 0,
            bytes: 24,
        }
    );
    assert_eq!(
        plan.risks,
        [
            Risk::Overwrite(loc("/dest/a.txt")),
            Risk::Overwrite(loc("/dest/b.txt")),
        ]
    );
    assert!(plan.conflicts.is_empty());
}

#[test]
fn an_empty_write_many_plan_is_refused() {
    assert_eq!(write_many_plan(&[]), Err(WriteManyError::Empty));
}

#[test]
fn a_target_used_twice_is_refused() {
    let writes = [
        write_target("/dest/a.txt", "/dest/.a.txt.tmp", 11),
        write_target("/dest/a.txt", "/dest/.other.txt.tmp", 13),
    ];
    assert_eq!(
        write_many_plan(&writes),
        Err(WriteManyError::Duplicate(loc("/dest/a.txt")))
    );
}

#[test]
fn a_staged_path_used_twice_is_refused() {
    let writes = [
        write_target("/dest/a.txt", "/dest/.a.txt.tmp", 11),
        write_target("/dest/b.txt", "/dest/.a.txt.tmp", 13),
    ];
    assert_eq!(
        write_many_plan(&writes),
        Err(WriteManyError::Duplicate(loc("/dest/.a.txt.tmp")))
    );
}

#[test]
fn a_staged_path_that_is_another_target_is_refused() {
    let writes = [
        write_target("/dest/a.txt", "/dest/.a.txt.tmp", 11),
        write_target("/dest/b.txt", "/dest/a.txt", 13),
    ];
    assert_eq!(
        write_many_plan(&writes),
        Err(WriteManyError::Duplicate(loc("/dest/a.txt")))
    );
}

#[test]
fn a_target_used_twice_in_another_case_is_refused() {
    let writes = [
        write_target(r"C:\Work\a.txt", r"C:\Work\.a.txt.tmp", 11),
        write_target(r"c:\work\A.TXT", r"c:\work\.other.txt.tmp", 13),
    ];
    assert_eq!(
        write_many_plan(&writes),
        Err(WriteManyError::Duplicate(loc(r"c:\work\A.TXT")))
    );
}

#[test]
fn a_many_step_plan_round_trips_through_json() {
    let plan = write_many_plan(&[
        write_target("/dest/a.txt", "/dest/.a.txt.tmp", 11),
        write_target("/dest/b.txt", "/dest/.b.txt.tmp", 13),
    ])
    .unwrap();
    let json = serde_json::to_value(&plan).unwrap();
    assert_eq!(json["steps"].as_array().unwrap().len(), 2);
    assert_eq!(serde_json::from_value::<OperationPlan>(json).unwrap(), plan);
}
