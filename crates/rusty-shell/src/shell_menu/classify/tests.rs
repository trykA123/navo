use rusty_core::shell_menu::{Group, ItemId, ItemKind, Submenu};

use super::{RawItem, guess_group, is_unfilled, to_menu_item};

fn raw(command: u32, text: &str, verb: Option<&str>) -> RawItem {
    RawItem {
        command,
        text: text.to_owned(),
        verb: verb.map(str::to_owned),
        enabled: true,
        checked: false,
        default: false,
        separator: false,
        icon: None,
    }
}

fn separator() -> RawItem {
    RawItem {
        separator: true,
        ..raw(0, "", None)
    }
}

#[test]
fn open_verbs_go_to_the_open_group() {
    for verb in ["open", "OpenAs", "edit", "opennewwindow", "runas"] {
        assert_eq!(guess_group(Some(verb), "Anything"), Group::Open, "{verb}");
    }
}

#[test]
fn the_open_with_submenu_goes_to_the_open_group() {
    assert_eq!(guess_group(None, "Open wit&h"), Group::Open);
}

#[test]
fn clipboard_and_file_verbs_go_to_the_edit_group() {
    for verb in ["copy", "cut", "delete", "rename", "paste", "link"] {
        assert_eq!(guess_group(Some(verb), "Anything"), Group::Edit, "{verb}");
    }
}

#[test]
fn share_verbs_go_to_the_share_group() {
    assert_eq!(
        guess_group(Some("Windows.ModernShare"), "Share"),
        Group::Share
    );
    assert_eq!(guess_group(Some("sendto"), "Send to"), Group::Share);
}

#[test]
fn share_texts_without_a_verb_go_to_the_share_group() {
    assert_eq!(guess_group(None, "&Send to"), Group::Share);
    assert_eq!(guess_group(None, "Give access to"), Group::Share);
}

#[test]
fn unknown_items_go_to_the_extensions_group() {
    assert_eq!(guess_group(None, "7-Zip"), Group::Extensions);
    assert_eq!(
        guess_group(
            Some("{9F156763-7844-4DC4-B2B1-901F640F5155}"),
            "Open in Terminal"
        ),
        Group::Extensions
    );
    assert_eq!(
        guess_group(Some("properties"), "&Properties"),
        Group::Extensions
    );
}

#[test]
fn a_submenu_with_one_blank_child_is_unfilled() {
    assert!(is_unfilled(&[raw(9, "", None)]));
}

#[test]
fn a_submenu_with_no_children_is_unfilled() {
    assert!(is_unfilled(&[]));
}

#[test]
fn separators_alone_do_not_fill_a_submenu() {
    assert!(is_unfilled(&[separator(), raw(9, "  ", None)]));
}

#[test]
fn a_submenu_with_a_named_child_is_filled() {
    assert!(!is_unfilled(&[raw(9, "", None), raw(10, "&Desktop", None)]));
}

#[test]
fn text_is_cleaned_and_the_verb_kept_in_the_id() {
    let item = to_menu_item(raw(12, "&Copy\tCtrl+C", Some("copy")), None, None);
    assert_eq!(item.text, "Copy");
    assert_eq!(item.access_key, Some('C'));
    assert_eq!(item.accelerator.as_deref(), Some("Ctrl+C"));
    assert_eq!(
        item.id,
        ItemId::Shell {
            command: 12,
            verb: Some("copy".to_owned())
        }
    );
    assert_eq!(item.group, Group::Edit);
    assert_eq!(item.kind, ItemKind::Command);
}

#[test]
fn state_flags_are_carried_over() {
    let item = to_menu_item(
        RawItem {
            enabled: false,
            checked: true,
            default: true,
            ..raw(3, "Open", Some("open"))
        },
        None,
        None,
    );
    assert!(!item.enabled);
    assert!(item.checked);
    assert!(item.default);
}

#[test]
fn a_raw_separator_becomes_a_separator() {
    assert!(to_menu_item(separator(), None, None).is_separator());
}

#[test]
fn submenu_children_take_the_parent_group() {
    let item = to_menu_item(raw(40, "Desktop", None), None, Some(Group::Share));
    assert_eq!(item.group, Group::Share);
}

#[test]
fn a_submenu_kind_is_kept() {
    let item = to_menu_item(raw(0, "Send to", None), Some(Submenu::Lazy), None);
    assert_eq!(item.kind, ItemKind::Submenu(Submenu::Lazy));
    assert_eq!(item.group, Group::Share);
}
