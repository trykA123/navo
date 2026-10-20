use rusty_core::{DropOp, PaneId};

use super::{DragState, FileDrag, Hover, Mark};

fn state(count: usize, op: Option<DropOp>, refusal: Option<&str>) -> DragState {
    DragState {
        count,
        first_name: "report.pdf".into(),
        hover: Some(Hover {
            target_label: "Projects".into(),
            op,
            refusal: refusal.map(Into::into),
        }),
        keys_line: true,
    }
}

#[test]
fn each_operation_names_itself_and_its_target() {
    let head = |op| state(3, Some(op), None).lines().head;
    assert_eq!(head(DropOp::Move), "Move 3 items to Projects");
    assert_eq!(head(DropOp::Copy), "Copy 3 items to Projects");
    assert_eq!(head(DropOp::Link), "Create 3 shortcuts in Projects");
}

#[test]
fn one_item_reads_singular() {
    assert_eq!(
        state(1, Some(DropOp::Copy), None).lines().head,
        "Copy 1 item to Projects"
    );
    assert_eq!(
        state(1, Some(DropOp::Link), None).lines().head,
        "Create 1 shortcut in Projects"
    );
}

#[test]
fn a_refused_target_shows_the_reason_and_the_refused_mark() {
    let lines = state(2, None, Some("Already in this folder")).lines();
    assert_eq!(lines.mark, Mark::Refused);
    assert_eq!(lines.head, "Already in this folder");
    assert_eq!(lines.keys, None);
}

#[test]
fn the_keys_line_follows_the_flag() {
    let mut shown = state(2, Some(DropOp::Move), None);
    assert!(shown.lines().keys.is_some());
    shown.keys_line = false;
    assert_eq!(shown.lines().keys, None);
}

#[test]
fn off_a_target_the_badge_names_what_is_carried() {
    let mut carry = state(1, None, None);
    carry.hover = None;
    assert_eq!(carry.lines().head, "report.pdf");
    carry.count = 4;
    assert_eq!(carry.lines().head, "4 items");
    assert_eq!(
        FileDrag {
            pane: PaneId::new(1),
            row: 2,
            name: "b.txt".into(),
        }
        .row,
        2
    );
}
