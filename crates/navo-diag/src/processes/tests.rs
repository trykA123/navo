// Tests for the process scan's name and parent rules, which hold on every platform.

use super::{is_other, unreadable_counts};

#[test]
fn an_unreadable_image_blocks_when_its_name_looks_like_navo() {
    assert!(unreadable_counts("navo.exe", "navo.exe", true));
    assert!(unreadable_counts("Navo-Index.exe", "navo.exe", true));
    assert!(unreadable_counts("renamed.exe", "renamed.exe", true));
    assert!(!unreadable_counts("csrss.exe", "navo.exe", true));
}

#[test]
fn an_unreadable_navo_image_in_another_session_does_not_block() {
    assert!(!unreadable_counts("navo.exe", "navo.exe", false));
    assert!(!unreadable_counts("Navo-Index.exe", "navo.exe", false));
}

#[test]
fn a_child_process_does_not_block_a_restart() {
    let own = 4000;
    assert!(!is_other(own, 1, own));
    assert!(!is_other(4100, own, own));
    assert!(!is_other(0, 0, own));
    assert!(is_other(4200, 1234, own));
}
