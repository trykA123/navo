#![allow(clippy::unwrap_used, reason = "test assertions")]

use rusty_automotive::{
    CanId, DescriptionError, Endian, Watch,
    dbc::{DbcModel, Multiplexing, Number, SignalFormat, ValueTarget},
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
    parse_bytes(text.as_bytes())
}

fn parse_bytes(bytes: &[u8]) -> DbcModel {
    index_dbc(
        bytes,
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

fn id(raw: u32) -> CanId {
    CanId {
        raw,
        extended: false,
    }
}

#[test]
fn dbc_decodes_windows_1252_units_instead_of_dropping_the_signal() {
    let mut bytes = b"BO_ 42 Frame: 8 Sender\n SG_ Temp : 0|8@1+ (1,0) [0|255] \"".to_vec();
    bytes.extend_from_slice(&[0xB0, b'C', 0x80]);
    bytes.extend_from_slice(b"\" Receiver\n");
    let model = parse_bytes(&bytes);
    assert!(
        model.report.problems.is_empty(),
        "{:?}",
        model.report.problems
    );
    assert_eq!(model.signals(id(42)).unwrap()[0].unit, "\u{b0}C\u{20ac}");
}

#[test]
fn dbc_skips_the_independent_signal_pseudo_message_silently() {
    let model = parse(
        "BO_ 3221225472 VECTOR__INDEPENDENT_SIG_MSG: 0 Vector__XXX\n SG_ Loose : 0|8@1+ (1,0) [0|255] \"\" Vector__XXX\nBO_ 42 Frame: 8 Sender\n SG_ Kept : 0|8@1+ (1,0) [0|255] \"\" Receiver\nCM_ SG_ 3221225472 Loose \"note\";\nBA_ \"GenSigStartValue\" SG_ 3221225472 Loose 3;\nVAL_ 3221225472 Loose 0 \"Off\";\nSIG_VALTYPE_ 3221225472 Loose : 1;\n",
    );
    assert!(
        model.report.problems.is_empty(),
        "{:?}",
        model.report.problems
    );
    assert_eq!(model.messages.len(), 1);
    assert_eq!(model.messages[0].signals[0].name, "Kept");
    assert!(model.comments.is_empty());
    assert!(model.value_tables.is_empty());
}

#[test]
fn dbc_accepts_a_spaced_node_header_and_continued_node_lists() {
    let model = parse("BU_ : First Second\n Third\n\tFourth\nBO_ 42 Frame: 8 First\n");
    assert!(
        model.report.problems.is_empty(),
        "{:?}",
        model.report.problems
    );
    assert_eq!(model.nodes, ["First", "Second", "Third", "Fourth"]);
    assert_eq!(model.messages.len(), 1);
    let model = parse("BU_: First\n Second\nBS_:\nBO_ 42 Frame: 8 First\n");
    assert_eq!(model.nodes, ["First", "Second"]);
}

#[test]
fn dbc_reads_float_and_double_signal_value_types() {
    let model = parse(
        "BO_ 42 Frame: 8 Sender\n SG_ Plain : 0|8@1+ (1,0) [0|255] \"\" Receiver\n SG_ Single : 8|32@1+ (1,0) [0|0] \"\" Receiver\n SG_ Wide : 0|64@1+ (1,0) [0|0] \"\" Receiver\nSIG_VALTYPE_ 42 Single : 1;\nSIG_VALTYPE_ 42 Wide : 2;\n",
    );
    assert!(
        model.report.problems.is_empty(),
        "{:?}",
        model.report.problems
    );
    let formats: Vec<_> = model
        .signals(id(42))
        .unwrap()
        .iter()
        .map(|signal| signal.format)
        .collect();
    assert_eq!(
        formats,
        [
            SignalFormat::Integer,
            SignalFormat::Float,
            SignalFormat::Double
        ]
    );
}

#[test]
fn dbc_checks_ids_after_attribute_names_that_contain_spaces() {
    let model = parse("BO_ 42 Frame: 8 Sender\nBA_ \"Gen Msg Cycle\" BO_ 65578 10;\n");
    assert_eq!(model.report.problem_count, 1);
    assert_eq!(model.report.problems[0].reason, "invalid CAN message ID");
    assert_eq!(model.report.problems[0].line, 2);
}

#[test]
fn dbc_stores_no_cycle_time_for_a_zero_cycle() {
    let text = "BO_ 42 Zero: 8 Sender\nBO_ 43 Default: 8 Sender\nBA_DEF_DEF_ \"GenMsgCycleTime\" 20;\nBA_ \"GenMsgCycleTime\" BO_ 42 0;\n";
    let model = parse(text);
    assert_eq!(model.message(id(42)).unwrap().cycle_time_ms, None);
    assert_eq!(model.message(id(43)).unwrap().cycle_time_ms, Some(20.0));
    let model = parse("BO_ 42 Frame: 8 Sender\nBA_DEF_DEF_ \"GenMsgCycleTime\" 0;\n");
    assert_eq!(model.cycle_time_default_ms, None);
    assert_eq!(model.message(id(42)).unwrap().cycle_time_ms, None);
}

#[test]
fn dbc_does_not_count_the_signal_wrapper_message_toward_the_row_cap() {
    let signals = " SG_ Value : 0|8@1+ (1,0) [0|255] \"\" Receiver\n".repeat(90_000);
    let model = parse(&format!("BO_ 42 Frame: 8 Sender\n{signals}"));
    assert_eq!(model.report.problem_count, 0);
    assert_eq!(model.signals(id(42)).unwrap().len(), 90_000);
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
