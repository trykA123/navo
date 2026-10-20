#![allow(clippy::unwrap_used, reason = "test assertions")]

use rusty_automotive::{A2lModel, DescriptionError, Watch, index_a2l, sniff_a2l};
use std::sync::atomic::{AtomicBool, Ordering};

const FIXTURE: &str = r#"ASAP2_VERSION 1 71
/begin PROJECT Invented "Fixture"
/begin MODULE First ""
/begin MEASUREMENT Speed "A measurement" UWORD Velocity 1 0 0 250
 ECU_ADDRESS 4096
 /begin IF_DATA XCP ignored payload /end IF_DATA
/end MEASUREMENT
/begin MEASUREMENT Virtual "" FLOAT32_IEEE NO_COMPU_METHOD 1 0 -1 1
 PHYS_UNIT "ratio"
/end MEASUREMENT
/begin CHARACTERISTIC Limit "A parameter" VALUE 8192 Scalar 0 Velocity 0 300
/end CHARACTERISTIC
/begin COMPU_METHOD Velocity "" IDENTICAL "%6.2" "km/h"
/end COMPU_METHOD
/begin IF_DATA CCP payload /end IF_DATA
/end MODULE
/begin MODULE Second ""
/begin MEASUREMENT Speed "" UBYTE Velocity 1 0 0 100
/end MEASUREMENT
/begin COMPU_METHOD Velocity "" IDENTICAL "%3.0" "percent"
/end COMPU_METHOD
/end MODULE
/end PROJECT
"#;

fn parse(text: &str) -> A2lModel {
    index_a2l(
        text.as_bytes(),
        Watch {
            cancel: &AtomicBool::new(false),
            progress: &mut |_| {},
        },
    )
    .unwrap()
}

#[test]
fn a2l_maps_measurements_characteristics_units_modules_and_interfaces() {
    let model = parse(FIXTURE);
    assert!(
        model.report.problems.is_empty(),
        "{:?}",
        model.report.problems
    );
    assert_eq!(model.project.as_deref(), Some("Invented"));
    assert_eq!(
        model
            .modules
            .iter()
            .map(|module| module.name.as_str())
            .collect::<Vec<_>>(),
        ["First", "Second"]
    );
    let row = &model.measurements[0];
    assert_eq!(
        (
            row.name.as_str(),
            row.address,
            row.data_type.as_str(),
            row.conversion.as_str(),
            row.unit.as_deref()
        ),
        ("Speed", Some(4096), "UWORD", "Velocity", Some("km/h"))
    );
    assert_eq!(
        (row.lower_limit, row.upper_limit, row.module, row.line),
        (0.0, 250.0, 0, 4)
    );
    assert_eq!(model.measurements[1].address, None);
    assert_eq!(model.measurements[1].unit.as_deref(), Some("ratio"));
    assert_eq!(model.measurements[2].unit.as_deref(), Some("percent"));
    let row = &model.characteristics[0];
    assert_eq!(
        (row.address, row.record_layout.as_str(), row.unit.as_deref()),
        (8192, "Scalar", Some("km/h"))
    );
    assert_eq!((row.lower_limit, row.upper_limit), (0.0, 300.0));
    assert_eq!(model.compu_methods.len(), 2);
    assert!(model.modules[0].interface_data.xcp);
    assert!(model.modules[0].interface_data.ccp);
    assert!(!model.modules[1].interface_data.present);
}

#[test]
fn a2l_keeps_rows_after_a_broken_block_and_reports_source_lines() {
    let text = "/begin PROJECT P \"\"\n/begin MODULE M \"\"\n/begin MEASUREMENT Bad \"\" INVALID Conv 1 0 0 1\n/end MEASUREMENT\n/begin MEASUREMENT Good \"\" UBYTE Conv 1 0 0 1\n/end MEASUREMENT\n/end MODULE\n/end PROJECT\n";
    let model = parse(text);
    assert_eq!(model.measurements.len(), 1);
    assert_eq!(model.measurements[0].name, "Good");
    assert_eq!(model.report.problems[0].line, 3);
}

#[test]
fn a2l_recovers_a_missing_row_end_and_caps_problems() {
    let body = "/begin MEASUREMENT Bad \"\" INVALID Conv 1 0 0 1 /end MEASUREMENT\n".repeat(1100);
    let model = parse(&format!(
        "/begin PROJECT P \"\" /begin MODULE M \"\"\n{body}/begin MEASUREMENT Missing \"\" UBYTE Conv 1 0 0 1\n/begin MEASUREMENT Good \"\" UBYTE Conv 1 0 0 1 /end MEASUREMENT\n/end MODULE /end PROJECT"
    ));
    assert_eq!(model.measurements.len(), 1);
    assert_eq!(model.report.problems.len(), 1000);
    assert!(model.report.problem_count > 1000);
}

#[test]
fn a2l_ignores_block_markers_in_strings_and_comments_and_does_not_load_includes() {
    let text = "/begin PROJECT P \"\" /begin MODULE M \"\"\n/* /begin MEASUREMENT Fake */\n/include \"missing-invented.a2l\"\n/begin MEASUREMENT Good \"/begin PROJECT Fake\" UBYTE Conv 1 0 0 1 /end MEASUREMENT\n/end MODULE /end PROJECT";
    let model = parse(text);
    assert_eq!(model.measurements.len(), 1);
    assert_eq!(model.report.problems.len(), 1);
    assert_eq!(model.report.problems[0].line, 3);
}

#[test]
fn a2l_cancels_during_a_large_comment_and_at_completion() {
    let cancel = AtomicBool::new(false);
    let text = format!("/*{}*/", "x".repeat(1_000_000));
    let result = index_a2l(
        text.as_bytes(),
        Watch {
            cancel: &cancel,
            progress: &mut |offset| {
                if offset >= 65_536 {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
        },
    );
    assert!(matches!(result, Err(DescriptionError::Cancelled)));
    let cancel = AtomicBool::new(false);
    let result = index_a2l(
        FIXTURE.as_bytes(),
        Watch {
            cancel: &cancel,
            progress: &mut |offset| {
                if offset == FIXTURE.len() as u64 {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
        },
    );
    assert!(matches!(result, Err(DescriptionError::Cancelled)));
}

#[test]
fn a2l_sniff_accepts_headers_and_rejects_other_formats() {
    for text in ["ASAP2_VERSION 1 71", "/begin PROJECT P \"\"", FIXTURE] {
        assert!(sniff_a2l(text.as_bytes()));
    }
    for text in [
        "plain text",
        "ASAP2_VERSIONED",
        "BO_ 42 Frame: 8 Sender",
        "/begin MODULE M",
    ] {
        assert!(!sniff_a2l(text.as_bytes()));
    }
}

#[test]
fn a2l_reports_the_exact_line_of_a_broken_optional_field() {
    let model = parse(
        "/begin PROJECT P \"\" /begin MODULE M \"\"\n/begin MEASUREMENT Bad \"\" UBYTE Conv 1 0 0 1\n ECU_ADDRESS invalid\n/end MEASUREMENT\n/end MODULE /end PROJECT",
    );
    assert!(model.measurements.is_empty());
    assert_eq!(model.report.problems[0].line, 3);
}

#[test]
fn a2l_reports_lines_after_skipped_interface_data() {
    let prefix = "/begin PROJECT P \"\" /begin MODULE M \"\"\n/begin MEASUREMENT Bad \"\" UBYTE Conv 1 0 0 1\n/begin IF_DATA XCP\n";
    let payload = "payload\n".repeat(300_000);
    let model = parse(&format!(
        "{prefix}{payload}/end IF_DATA\nECU_ADDRESS invalid\n/end MEASUREMENT\n/end MODULE /end PROJECT"
    ));
    assert_eq!(model.report.problem_count, 1);
    assert_eq!(model.report.problems[0].line, 300_005);
}

#[test]
fn a2l_keeps_rows_with_library_warnings_and_ignores_interface_payload_blocks() {
    let text = "/begin PROJECT P \"\" /begin MODULE M \"\"\n/begin MEASUREMENT Good \"\" UBYTE Conv 1 0 0 1\n UNKNOWN_OPTION 7\n /begin IF_DATA XCP /begin MEASUREMENT payload /end MEASUREMENT /end IF_DATA\n/end MEASUREMENT\n/end MODULE /end PROJECT";
    let model = parse(text);
    assert_eq!(model.measurements.len(), 1);
    assert_eq!(model.measurements[0].name, "Good");
    assert_eq!(model.report.problems.len(), 1);
    assert_eq!(model.report.problems[0].line, 3);
    assert!(model.modules[0].interface_data.xcp);
}

#[test]
fn sniffers_ignore_markers_in_comments_and_strings() {
    for text in [
        "/* ASAP2_VERSION 1 71 */",
        "\"/begin PROJECT P\"",
        "// /begin PROJECT P",
    ] {
        assert!(!sniff_a2l(text.as_bytes()));
    }
    for text in [
        "/*\nBO_ 42 Frame: 8 Sender\n*/",
        "\"BU_: Sender\"",
        "// VERSION \"invented\"",
    ] {
        assert!(!rusty_automotive::sniff_dbc(text.as_bytes()));
    }
    assert!(rusty_automotive::sniff_dbc(b"VERSION \"invented\"\n"));
}

proptest::proptest! {
    #[test]
    fn description_parsers_never_panic_on_arbitrary_bytes(bytes in proptest::collection::vec(proptest::num::u8::ANY, 0..4096)) {
        let cancel = AtomicBool::new(false);
        let _ = index_a2l(bytes.as_slice(), Watch { cancel: &cancel, progress: &mut |_| {} });
        let _ = rusty_automotive::index_dbc(bytes.as_slice(), Watch { cancel: &cancel, progress: &mut |_| {} });
    }

    #[test]
    fn library_parsers_report_malformed_row_payloads(payload in "[ -~]{0,512}") {
        let _ = parse(&format!("/begin PROJECT P \"\" /begin MODULE M \"\" /begin MEASUREMENT {payload} /end MEASUREMENT /end MODULE /end PROJECT"));
        let text = format!("BO_ 42 Frame: 8 Sender\n SG_ {payload}\n");
        let _ = rusty_automotive::index_dbc(text.as_bytes(), Watch { cancel: &AtomicBool::new(false), progress: &mut |_| {} });
    }
}
