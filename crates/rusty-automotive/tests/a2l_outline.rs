#![allow(clippy::unwrap_used, reason = "test assertions")]

use rusty_automotive::{A2lModel, ItemRef, Watch, a2l, index_a2l};
use std::sync::atomic::AtomicBool;

const FIXTURE: &str = r#"ASAP2_VERSION 1 71
/begin PROJECT Invented ""
/begin MODULE Demo ""
/begin IF_DATA XCP /end IF_DATA
/begin CHARACTERISTIC Scalar "" VALUE 0x10 Layout 0 NO_COMPU_METHOD 0 1 /end CHARACTERISTIC
/begin CHARACTERISTIC Surface "" MAP 0x20AB Layout 0 Shared 0 1 /end CHARACTERISTIC
/begin CHARACTERISTIC Line "" CURVE 0x30 Layout 0 NO_COMPU_METHOD 0 1 /end CHARACTERISTIC
/begin CHARACTERISTIC Block "" VAL_BLK 0x40 Layout 0 NO_COMPU_METHOD 0 1 /end CHARACTERISTIC
/begin CHARACTERISTIC Text "" ASCII 0x50 Layout 0 NO_COMPU_METHOD 0 1 /end CHARACTERISTIC
/begin CHARACTERISTIC Cube "" CUBOID 0x60 Layout 0 Absent 0 1 /end CHARACTERISTIC
/begin CHARACTERISTIC SecondSurface "" MAP 0x70 Layout 0 NO_COMPU_METHOD 0 1 /end CHARACTERISTIC
/begin MEASUREMENT Speed "" UWORD Shared 1 0 0 100 /end MEASUREMENT
/begin MEASUREMENT Missing "" UBYTE Unknown 1 0 0 1 /end MEASUREMENT
/begin COMPU_METHOD Shared "" LINEAR "%6.2" "rpm" COEFFS_LINEAR 1 0 /end COMPU_METHOD
/begin COMPU_METHOD Unused "" IDENTICAL "%6.2" "" /end COMPU_METHOD
/end MODULE
/end PROJECT
"#;

fn parse() -> A2lModel {
    let model = index_a2l(
        FIXTURE.as_bytes(),
        Watch {
            cancel: &AtomicBool::new(false),
            progress: &mut |_| {},
        },
    )
    .unwrap();
    assert_eq!(model.report.problem_count, 0, "{:?}", model.report.problems);
    model
}

#[test]
fn groups_and_kind_subgroups_follow_order_with_counts_and_original_indexes() {
    use ItemRef::{Characteristic, Compu, Measurement};
    let outline = a2l::outline(&parse());
    let rows: Vec<_> = outline
        .rows
        .iter()
        .map(|row| (row.depth, row.group, row.item, &*row.label, &*row.aux))
        .collect();
    assert_eq!(
        rows,
        [
            (0, false, None, "Module Demo · XCP", ""),
            (0, true, None, "Characteristics (7)", ""),
            (1, true, None, "MAP", "2"),
            (2, false, Some(Characteristic(1)), "Surface", "0x20AB"),
            (2, false, Some(Characteristic(6)), "SecondSurface", "0x70"),
            (1, true, None, "CURVE", "1"),
            (2, false, Some(Characteristic(2)), "Line", "0x30"),
            (1, true, None, "VAL_BLK", "1"),
            (2, false, Some(Characteristic(3)), "Block", "0x40"),
            (1, true, None, "VALUE", "1"),
            (2, false, Some(Characteristic(0)), "Scalar", "0x10"),
            (1, true, None, "ASCII", "1"),
            (2, false, Some(Characteristic(4)), "Text", "0x50"),
            (1, true, None, "CUBOID", "1"),
            (2, false, Some(Characteristic(5)), "Cube", "0x60"),
            (0, true, None, "Measurements (2)", ""),
            (1, false, Some(Measurement(0)), "Speed", "UWORD rpm"),
            (1, false, Some(Measurement(1)), "Missing", "UBYTE"),
            (0, true, None, "Compu methods (2)", ""),
            (1, false, Some(Compu(0)), "Shared", "LINEAR · 2"),
            (1, false, Some(Compu(1)), "Unused", "IDENTICAL · 0"),
        ]
    );
}

#[test]
fn links_list_shared_users_and_missing_conversions_but_skip_no_compu_method() {
    let links = a2l::links(&parse());
    assert_eq!(
        links.users,
        [
            vec![ItemRef::Characteristic(1), ItemRef::Measurement(0)],
            vec![]
        ]
    );
    assert_eq!(
        links.missing,
        [ItemRef::Characteristic(5), ItemRef::Measurement(1)]
    );
}

#[test]
fn source_lines_and_lowercase_search_survive_grouping() {
    let outline = a2l::outline(&parse());
    assert_eq!(outline.rows[0].line, 3);
    for (line, row) in [(5, 10), (6, 3), (11, 4), (12, 16), (14, 19)] {
        assert_eq!(outline.rows[row].line, line);
        assert_eq!(outline.item_at_line(line), Some(row));
    }
    for row in &outline.rows {
        assert_eq!(
            &*row.search,
            format!("{} {}", row.label, row.aux).to_lowercase()
        );
    }
    assert_eq!(outline.parent(3), Some(2));
    assert_eq!(outline.parent(2), Some(1));
    assert_eq!(outline.parent(16), Some(15));
}

#[test]
fn module_info_selects_protocol_and_empty_groups_are_omitted() {
    let mut model = parse();
    model.characteristics.clear();
    model.measurements.clear();
    model.compu_methods.clear();
    model.modules[0].interface_data.xcp = false;
    model.modules[0].interface_data.ccp = true;
    let outline = a2l::outline(&model);
    assert_eq!(outline.rows.len(), 1);
    assert_eq!(&*outline.rows[0].label, "Module Demo · CCP");
    model.modules[0].interface_data.ccp = false;
    assert_eq!(&*a2l::outline(&model).rows[0].label, "Module Demo");
    assert!(a2l::outline(&A2lModel::default()).rows.is_empty());
}
