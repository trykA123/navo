use super::Group;

pub(super) const GROUPS: [Group; 1] = [Group {
    context: "PopupMenu",
    commands: &[
        ("popup_menu::MenuUp", "popup_menu.up", "cmd-popup-menu-up"),
        (
            "popup_menu::MenuDown",
            "popup_menu.down",
            "cmd-popup-menu-down",
        ),
        (
            "popup_menu::MenuChoose",
            "popup_menu.choose",
            "cmd-popup-menu-choose",
        ),
        (
            "popup_menu::MenuClose",
            "popup_menu.close",
            "cmd-popup-menu-close",
        ),
    ],
}];
