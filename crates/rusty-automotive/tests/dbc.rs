#![allow(clippy::unwrap_used, reason = "test assertions")]

use rusty_automotive::{
    CanId, DescriptionError, Endian, Watch,
    dbc::{DbcModel, Multiplexing, Number, ValueTarget},
    index_dbc, sniff_dbc,
};
use std::sync::atomic::{AtomicBool, Ordering};

const FIXTURE: &str = r#"VERSION "invented"
NS_ :
 CM_
 BA_DEF_
 BA_
 VAL_
BS_:
BU_: Sender Receiver
BO_ 42 Frame: 8 Sender
 SG_ Mode M : 0|8@1+ (1,0) [0|255] "" Receiver
 SG_ Signed m3 : 15|16@0- (0.5,-10) [-100|100] "deg" Receiver,Sender
BO_ 2147483690 Extended: 8 Sender
 SG_ Unsigned : 0|64@1+ (1,0) [0|18446744073709551615] "ticks" Receiver
VAL_TABLE_ Modes 0 "Off" 3 "On";
VAL_ 42 Mode 0 "Off" 3 "On";
CM_ "Invented database";
CM_ BU_ Sender "A node";
CM_ BO_ 42 "A message";
CM_ SG_ 42 Signed "First line
second line";
BA_DEF_ BO_ "GenMsgCycleTime" INT 0 1000;
BA_DEF_DEF_ "GenMsgCycleTime" 20;
BA_ "GenMsgCycleTime" BO_ 42 10;
SG_MUL_VAL_ 42 Signed Mode 3-3, 5-8;
"#;

fn parse(text: &str) -> DbcModel {
    index_dbc(
        text.as_bytes(),
        Watch {
            cancel: &AtomicBool::new(false),
            progress: &mut |_| {},
        },
    )
    .unwrap()
}

#[test]
fn dbc_preserves_message_ids_nodes_senders_and_cycles() {
    let model = parse(FIXTURE);
    assert!(
        model.report.problems.is_empty(),
        "{:?}",
        model.report.problems
    );
    assert_eq!(model.nodes, ["Sender", "Receiver"]);
    let message = model
        .message(CanId {
            raw: 42,
            extended: false,
        })
        .unwrap();
    assert_eq!(message.name, "Frame");
    assert_eq!(message.dlc, 8);
    assert_eq!(message.sender.as_deref(), Some("Sender"));
    assert_eq!(message.cycle_time_ms, Some(10.0));
    let extended = model
        .message(CanId {
            raw: 42,
            extended: true,
        })
        .unwrap();
    assert_eq!(extended.cycle_time_ms, Some(20.0));
}

#[test]
fn dbc_preserves_multiplexed_signed_and_big_endian_signals() {
    let model = parse(FIXTURE);
    let signals = model
        .signals(CanId {
            raw: 42,
            extended: false,
        })
        .unwrap();
    assert_eq!(signals[0].multiplexing, Multiplexing::Switch);
    let signal = &signals[1];
    assert_eq!(signal.start_bit, 15);
    assert_eq!(signal.length, 16);
    assert_eq!(signal.byte_order, Endian::Big);
    assert!(signal.signed);
    assert_eq!(signal.factor, 0.5);
    assert_eq!(signal.offset, -10.0);
    assert_eq!(signal.min, Number::Signed(-100));
    assert_eq!(signal.max, Number::Unsigned(100));
    assert_eq!(signal.unit, "deg");
    assert_eq!(signal.receivers, ["Receiver", "Sender"]);
    assert_eq!(signal.multiplexing, Multiplexing::Value(3));
    let extended = model
        .signals(CanId {
            raw: 42,
            extended: true,
        })
        .unwrap();
    assert_eq!(extended[0].max, Number::Unsigned(u64::MAX));
}

#[test]
fn dbc_preserves_value_tables_comments_and_extended_ranges() {
    let model = parse(FIXTURE);
    assert_eq!(model.value_tables.len(), 2);
    assert_eq!(
        model.value_tables[0].target,
        ValueTarget::Table("Modes".into())
    );
    assert_eq!(model.value_tables[1].values[1].label, "On");
    assert_eq!(model.comments.len(), 4);
    assert!(model.comments[3].text.contains("second line"));
    assert_eq!(model.multiplex_ranges[0].ranges, [3..=3, 5..=8]);
}

#[test]
fn dbc_keeps_good_rows_after_malformed_records_and_caps_problems() {
    let text = format!(
        "BO_ 42 First: 8 Sender\n{}BO_ 43 Last: 8 Sender\n SG_ Value : 0|8@1+ (1,0) [0|255] \"\" Receiver\n",
        " SG_ broken\n".repeat(1100)
    );
    let model = parse(&text);
    assert_eq!(model.messages.len(), 2);
    assert_eq!(model.messages[1].signals.len(), 1);
    assert_eq!(model.report.problems.len(), 1000);
    assert_eq!(model.report.problem_count, 1100);
    assert_eq!(model.report.problems[0].line, 2);
}

#[test]
fn dbc_rejects_ids_that_the_library_would_truncate() {
    let model = parse(
        "BO_ 65578 Invalid: 8 Sender\n SG_ Value : 0|8@1+ (1,0) [0|255] \"\" Receiver\nBO_ 42 Valid: 8 Sender\n",
    );
    assert_eq!(model.messages.len(), 1);
    assert_eq!(model.messages[0].name, "Valid");
    assert_eq!(model.report.problems[0].line, 1);
}

#[test]
fn dbc_reports_progress_and_cancels_in_a_long_record() {
    let text = format!("CM_ \"{}\";", "x".repeat(1_000_000));
    let cancel = AtomicBool::new(false);
    let mut last = 0;
    let result = index_dbc(
        text.as_bytes(),
        Watch {
            cancel: &cancel,
            progress: &mut |offset| {
                assert!(offset >= last);
                last = offset;
                if offset >= 65_536 {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
        },
    );
    assert!(matches!(result, Err(DescriptionError::Cancelled)));
    assert_eq!(last, 65_536);
}

#[test]
fn dbc_sniff_accepts_database_content_and_rejects_other_text() {
    assert!(sniff_dbc(FIXTURE.as_bytes()));
    assert!(sniff_dbc(b"BO_ 42 Frame: 8 Sender\n"));
    for text in [
        "plain text",
        "VERSION",
        "BO_ is mentioned here",
        "/begin PROJECT P",
        "BU_ something",
    ] {
        assert!(!sniff_dbc(text.as_bytes()), "{text}");
    }
}
