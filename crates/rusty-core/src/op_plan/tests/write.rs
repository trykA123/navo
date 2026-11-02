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
