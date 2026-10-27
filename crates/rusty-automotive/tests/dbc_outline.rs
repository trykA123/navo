#![allow(clippy::unwrap_used, reason = "test assertions")]

use rusty_automotive::{DbcModel, ItemRef, Outline, Watch, dbc, index_dbc};
use std::sync::atomic::AtomicBool;

const FIXTURE: &str = r#"VERSION "invented outline"
BU_: Alpha Beta
BO_ 10 FirstBeta: 8 Beta
 SG_ Mode M : 0|8@1+ (1,0) [0|255] "" Alpha
 SG_ Reading m1 : 16|3@1+ (1,0) [0|7] "Nm" Alpha
BO_ 416 AlphaFrame: 8 Alpha
BO_ 2147483690 Extended: 8 Vector__XXX
BO_ 12 Unknown: 8 Missing
BO_ 13 SecondBeta: 8 Beta
BA_DEF_ BO_ "GenMsgCycleTime" INT 0 1000;
BA_ "GenMsgCycleTime" BO_ 416 20;
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

#[test]
fn groups_follow_node_order_and_messages_keep_file_order_with_original_indexes() {
    use ItemRef::{Message, Node, Signal};
    let outline = dbc::outline(&parse());
    let rows: Vec<_> = outline
        .rows
        .iter()
        .map(|row| {
            (
                row.depth,
                row.group,
                row.item,
                row.label.as_ref(),
                row.aux.as_ref(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            (0, true, Some(Node(0)), "Alpha", "1 msg"),
            (1, true, Some(Message(1)), "0x1A0 AlphaFrame", "20 ms"),
            (0, true, Some(Node(1)), "Beta", "2 msg"),
            (1, true, Some(Message(0)), "0xA FirstBeta", ""),
            (
                2,
                false,
                Some(Signal {
                    message: 0,
                    signal: 0
                }),
                "Mode",
                "0|8"
            ),
            (
                2,
                false,
                Some(Signal {
                    message: 0,
                    signal: 1
                }),
                "Reading",
                "16|3 Nm"
            ),
            (1, true, Some(Message(4)), "0xD SecondBeta", ""),
            (0, true, None, "No sender", "2 msg"),
            (1, true, Some(Message(2)), "0x2A Extended (extended)", ""),
            (1, true, Some(Message(3)), "0xC Unknown", ""),
        ]
    );
}

#[test]
fn signal_lines_and_nearest_items_follow_source_order_after_grouping() {
    let model = parse();
    assert_eq!(model.messages[0].signals[0].line, 4);
    assert_eq!(model.messages[0].signals[1].line, 5);
    let outline = dbc::outline(&model);
    for (line, row) in [
        (3, 3),
        (4, 4),
        (5, 5),
        (6, 1),
        (7, 8),
        (8, 9),
        (9, 6),
        (99, 6),
    ] {
        assert_eq!(outline.item_at_line(line), Some(row));
    }
    assert_eq!(Outline::default().item_at_line(10), None);
}

#[test]
fn parents_follow_depth_and_reject_rows_outside_the_outline() {
    let outline = dbc::outline(&parse());
    for (row, parent) in [
        (0, None),
        (1, Some(0)),
        (3, Some(2)),
        (4, Some(3)),
        (5, Some(3)),
        (8, Some(7)),
        (10, None),
        (usize::MAX, None),
    ] {
        assert_eq!(outline.parent(row), parent);
    }
}

#[test]
fn search_contains_lowercase_labels_auxiliary_text_and_hex_ids() {
    let outline = dbc::outline(&parse());
    assert!(outline.rows[1].search.contains("0x1a0 alphaframe"));
    assert!(outline.rows[1].search.contains("20 ms"));
    assert!(outline.rows[5].search.contains("reading"));
    assert!(outline.rows[5].search.contains("16|3 nm"));
    assert!(
        outline
            .rows
            .iter()
            .all(|row| row.search.as_ref() == row.search.to_lowercase())
    );
}

#[test]
fn empty_nodes_remain_and_no_sender_is_only_added_when_needed() {
    let mut model = parse();
    model.messages.clear();
    let outline = dbc::outline(&model);
    assert_eq!(outline.rows.len(), 2);
    assert!(outline.rows.iter().all(|row| row.aux.as_ref() == "0 msg"));
    model.nodes.clear();
    assert!(dbc::outline(&model).rows.is_empty());
}

#[test]
fn absent_empty_and_placeholder_senders_share_the_last_group() {
    let mut model = parse();
    model.messages.truncate(3);
    model.messages[0].sender = None;
    model.messages[1].sender = Some(String::new());
    model.nodes.push("Vector__XXX".into());
    let outline = dbc::outline(&model);
    let group = outline
        .rows
        .iter()
        .position(|row| row.label.as_ref() == "No sender")
        .unwrap();
    assert_eq!(outline.rows[group].aux.as_ref(), "3 msg");
    assert_eq!(outline.rows[group].item, None);
    assert_eq!(outline.parent(group), None);
}
