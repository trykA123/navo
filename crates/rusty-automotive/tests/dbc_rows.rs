#![allow(clippy::unwrap_used, reason = "test assertions")]

use rusty_automotive::{DbcModel, DescRow, Watch, dbc, index_dbc};
use std::sync::atomic::AtomicBool;

const FIXTURE: &str = r#"VERSION "invented rows"
BU_: Sender Receiver
BO_ 42 Frame: 8 Sender
 SG_ Mode M : 0|8@1+ (1,0) [0|255] "" Receiver
 SG_ Reading m1 : 15|16@0- (0.5,-10) [-100|100] "deg" Receiver,Sender
BO_ 2147483690 Extended: 8 Vector__XXX
 SG_ Mode : 0|8@1+ (1,0) [0|255] "" Receiver
VAL_TABLE_ Mode 9 "Unlinked";
VAL_ 42 Mode 0 "Off" 1 "On";
VAL_ 42 Reading 0 "Off" 1 "On" 2 "Error" 3 "Idle" 4 "Ready" 5 "Busy";
CM_ BO_ 42 "Invented message";
BA_DEF_ BO_ "GenMsgCycleTime" INT 0 1000;
BA_ "GenMsgCycleTime" BO_ 42 10;
"#;

fn parse() -> DbcModel {
    let model = index_dbc(
        FIXTURE.as_bytes(),
        Watch {
            cancel: &AtomicBool::new(false),
            progress: &mut |_| {},
        },
    )
    .unwrap();
    assert!(model.report.problems.is_empty(), "{:?}", model.report);
    model
}

fn cell<'a>(headers: &[&str], row: &'a DescRow, header: &str) -> &'a str {
    &row.cells[headers.iter().position(|name| *name == header).unwrap()]
}

#[test]
fn dbc_rows_keep_message_and_signal_order_without_extra_rows() {
    let (headers, rows) = dbc::rows(&parse());
    let order: Vec<_> = rows
        .iter()
        .map(|row| (row.depth, row.cells[0].as_ref()))
        .collect();
    assert_eq!(
        order,
        [
            (0, "Frame"),
            (1, "Mode"),
            (1, "Reading"),
            (0, "Extended"),
            (1, "Mode")
        ]
    );
    assert!(rows.iter().all(|row| row.cells.len() == headers.len()));
}

#[test]
fn dbc_message_rows_show_hex_ids_cycles_and_comments() {
    let (headers, rows) = dbc::rows(&parse());
    for (header, expected) in [
        ("ID", "0x2A"),
        ("DLC", "8"),
        ("Sender", "Sender"),
        ("Cycle time (ms)", "10"),
        ("Comment", "Invented message"),
        ("Start bit", ""),
        ("Values", ""),
    ] {
        assert_eq!(cell(&headers, &rows[0], header), expected);
    }
    assert_eq!(cell(&headers, &rows[3], "ID"), "0x2A (extended)");
    assert_eq!(cell(&headers, &rows[3], "Cycle time (ms)"), "");
    assert_eq!(cell(&headers, &rows[3], "Sender"), "");
    assert_eq!(cell(&headers, &rows[3], "Comment"), "");
}

#[test]
fn dbc_signal_rows_include_multiplexed_signal_fields() {
    let (headers, rows) = dbc::rows(&parse());
    for (header, expected) in [
        ("Name", "Reading"),
        ("ID", ""),
        ("DLC", ""),
        ("Sender", ""),
        ("Cycle time (ms)", ""),
        ("Comment", ""),
        ("Start bit", "15"),
        ("Length", "16"),
        ("Byte order", "Motorola"),
        ("Factor", "0.5"),
        ("Offset", "-10"),
        ("Unit", "deg"),
        ("Min", "-100"),
        ("Max", "100"),
        ("Receivers", "Receiver, Sender"),
    ] {
        assert_eq!(cell(&headers, &rows[2], header), expected);
    }
    assert_eq!(cell(&headers, &rows[1], "Byte order"), "Intel");
}

#[test]
fn dbc_values_use_signal_descriptions_and_truncate_after_four_pairs() {
    let (headers, rows) = dbc::rows(&parse());
    assert_eq!(cell(&headers, &rows[1], "Values"), "0=Off, 1=On");
    assert_eq!(
        cell(&headers, &rows[2], "Values"),
        "0=Off, 1=On, 2=Error, 3=Idle, …"
    );
    assert_eq!(cell(&headers, &rows[4], "Values"), "");
}

#[test]
fn dbc_search_is_the_lowercase_join_of_each_rows_cells() {
    let (_, rows) = dbc::rows(&parse());
    for row in rows {
        assert_eq!(row.search.as_ref(), row.cells.join(" ").to_lowercase());
    }
}
