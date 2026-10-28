use super::super::Group;

pub(super) const PATH_INPUT: Group = Group {
    context: "PathInput",
    commands: &[(
        "path_input::CancelPath",
        "path_input.cancel",
        "cmd-path-input-cancel",
    )],
};

pub(super) const DRIVE_MENU: Group = Group {
    context: "DriveMenu",
    commands: &[
        ("drive_menu::MenuUp", "drive_menu.up", "cmd-drive-menu-up"),
        (
            "drive_menu::MenuDown",
            "drive_menu.down",
            "cmd-drive-menu-down",
        ),
        (
            "drive_menu::MenuOpen",
            "drive_menu.open",
            "cmd-drive-menu-open",
        ),
        (
            "drive_menu::MenuClose",
            "drive_menu.close",
            "cmd-drive-menu-close",
        ),
    ],
};
