use super::Group;
use super::contexts::GLOBAL;

pub(super) const GROUPS: [Group; 2] = [
    Group {
        context: "FileList",
        commands: &[(
            "navo_window::RunUserCommand",
            "user_commands.run",
            "cmd-user-commands-run",
        )],
    },
    Group {
        context: GLOBAL,
        commands: &[(
            "navo_window::EditUserCommands",
            "user_commands.edit",
            "cmd-user-commands-edit",
        )],
    },
];
