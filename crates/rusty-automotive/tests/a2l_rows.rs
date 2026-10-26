#![allow(clippy::unwrap_used, reason = "test assertions")]

use rusty_automotive::{A2lModel, Watch, a2l, index_a2l};
use std::sync::atomic::AtomicBool;

const FIXTURE: &str = r#"ASAP2_VERSION 1 71
/begin PROJECT Invented ""
/begin MODULE Demo ""
/begin MEASUREMENT Zebra "" UWORD Rational 1 0 -10 200
 ECU_ADDRESS 0x12AB
/end MEASUREMENT
/begin MEASUREMENT Alpha "" UBYTE NO_COMPU_METHOD 1 0 0 1
/end MEASUREMENT
/begin CHARACTERISTIC Surface "" MAP 0x20CD Layout 0 Rational -5 100
 /begin AXIS_DESCR STD_AXIS InputX Linear 8 0 10 /end AXIS_DESCR
 /begin AXIS_DESCR STD_AXIS InputY Rational 16 -1 1 /end AXIS_DESCR
/end CHARACTERISTIC
/begin CHARACTERISTIC Line "" CURVE 4096 Layout 0 Linear 0 10
 /begin AXIS_DESCR STD_AXIS InputX Linear 8 0 10 /end AXIS_DESCR
/end CHARACTERISTIC
/begin COMPU_METHOD Rational "" RAT_FUNC "%6.2" "rpm"
 COEFFS 1 2 3 4 5 6
/end COMPU_METHOD
/begin COMPU_METHOD Linear "" LINEAR "%6.2" "ratio"
 COEFFS_LINEAR 2.5 -1
/end COMPU_METHOD
/end MODULE
/end PROJECT
"#;

fn parse(text: &str) -> A2lModel {
    let model = index_a2l(
        text.as_bytes(),
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
fn a2l_rows_group_items_in_source_order_with_counts_and_shared_columns() {
    let (columns, rows) = a2l::rows(&parse(FIXTURE));
    let labels: Vec<_> = rows.iter().map(|row| (row.depth, &*row.cells[0])).collect();
    assert_eq!(
        labels,
        [
            (0, "Measurements (2)"),
            (1, "Zebra"),
            (1, "Alpha"),
            (0, "Characteristics (2)"),
            (1, "Surface"),
            (1, "Line"),
            (0, "Conversions (2)"),
            (1, "Rational"),
            (1, "Linear"),
        ]
    );
    for row in &rows {
        assert_eq!(row.cells.len(), columns.len());
        assert_eq!(&*row.search, row.cells.join(" ").to_lowercase());
        if row.depth == 0 {
            assert!(row.cells[1..].iter().all(|cell| cell.is_empty()));
        }
    }
    let cell = |row: usize, column: &str| {
        &*rows[row].cells[columns.iter().position(|header| *header == column).unwrap()]
    };
    assert_eq!(cell(1, "Address"), "0x12AB");
    assert_eq!(cell(2, "Address"), "");
    assert_eq!(cell(1, "Data type"), "UWORD");
    assert_eq!(cell(1, "Conversion"), "Rational");
    assert_eq!(cell(1, "Unit"), "rpm");
    assert_eq!(cell(1, "Min"), "-10");
    assert_eq!(cell(1, "Max"), "200");
    assert_eq!(cell(4, "Address"), "0x20CD");
    assert_eq!(cell(4, "Kind"), "MAP 2 axes");
    assert_eq!(cell(5, "Kind"), "CURVE 1 axis");
    assert_eq!(cell(4, "Unit"), "");
    assert_eq!(cell(4, "Data type"), "");
    assert_eq!(cell(7, "Type"), "RAT_FUNC");
    assert_eq!(cell(7, "Unit"), "rpm");
    assert_eq!(cell(7, "Address"), "");
    assert_eq!(cell(7, "Min"), "");
}

#[test]
fn a2l_retains_axis_inputs_conversions_and_conversion_coefficients() {
    let model = parse(FIXTURE);
    let axes = &model.characteristics[0].axes;
    assert_eq!(axes.len(), 2);
    assert_eq!(
        (axes[0].input_quantity.as_str(), axes[0].conversion.as_str()),
        ("InputX", "Linear")
    );
    assert_eq!(
        (axes[1].input_quantity.as_str(), axes[1].conversion.as_str()),
        ("InputY", "Rational")
    );
    assert_eq!(
        model.compu_methods[0].coeffs,
        Some([1., 2., 3., 4., 5., 6.])
    );
    assert_eq!(model.compu_methods[0].coeffs_linear, None);
    assert_eq!(model.compu_methods[1].coeffs_linear, Some([2.5, -1.]));
    assert_eq!(model.compu_methods[1].coeffs, None);
}

#[test]
fn a2l_rows_omit_empty_groups() {
    let mut model = parse(FIXTURE);
    model.measurements.clear();
    model.compu_methods.clear();
    let (_, rows) = a2l::rows(&model);
    assert_eq!(rows.len(), 3);
    assert_eq!(&*rows[0].cells[0], "Characteristics (2)");
    model.characteristics.clear();
    assert!(a2l::rows(&model).1.is_empty());
}
