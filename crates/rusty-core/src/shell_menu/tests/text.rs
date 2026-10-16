use super::*;

#[test]
fn access_key_marker_is_stripped_and_taken() {
    let clean = clean_text("E&xtract All…");
    assert_eq!(clean.text, "Extract All…");
    assert_eq!(clean.access_key, Some('X'));
}

#[test]
fn double_ampersand_stays_literal() {
    let clean = clean_text("Tom && &Jerry");
    assert_eq!(clean.text, "Tom & Jerry");
    assert_eq!(clean.access_key, Some('J'));
}

#[test]
fn accelerator_moves_to_its_own_field() {
    let clean = clean_text("&Copy\tCtrl+C");
    assert_eq!(clean.text, "Copy");
    assert_eq!(clean.accelerator.as_deref(), Some("Ctrl+C"));
}

#[test]
fn plain_text_has_no_key_or_accelerator() {
    let clean = clean_text("Properties");
    assert_eq!(clean.text, "Properties");
    assert_eq!(clean.access_key, None);
    assert_eq!(clean.accelerator, None);
}

#[test]
fn trailing_ampersand_is_dropped() {
    assert_eq!(clean_text("Save &").text, "Save ");
}

#[test]
fn first_marker_wins() {
    assert_eq!(clean_text("&One &Two").access_key, Some('O'));
}

#[test]
fn item_built_from_raw_text_is_clean() {
    let item = MenuItem::new(
        ItemId::Shell {
            command: 7,
            verb: Some("cut".to_owned()),
        },
        "Cu&t\tCtrl+X",
        Group::Edit,
    );
    assert_eq!(item.text, "Cut");
    assert_eq!(item.access_key, Some('T'));
    assert_eq!(item.accelerator.as_deref(), Some("Ctrl+X"));
}
