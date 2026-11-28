//! Terminal module command registrations.

use super::Group;
use super::contexts::GLOBAL;

const TERMINAL: &str = "Terminal";

pub(super) const GROUPS: [Group; 2] = [
    Group {
        context: GLOBAL,
        commands: &[(
            "navo_window::ToggleTerminal",
            "terminal.toggle",
            "cmd-terminal-toggle",
        )],
    },
    Group {
        context: TERMINAL,
        commands: &[
            (
                "terminal::NewSession",
                "terminal.new_session",
                "cmd-terminal-new-session",
            ),
            (
                "terminal::OpenShellPicker",
                "terminal.open_shell_picker",
                "cmd-terminal-open-shell-picker",
            ),
            (
                "terminal::CloseSession",
                "terminal.close_session",
                "cmd-terminal-close-session",
            ),
            (
                "terminal::NextSession",
                "terminal.next_session",
                "cmd-terminal-next-session",
            ),
            (
                "terminal::PreviousSession",
                "terminal.previous_session",
                "cmd-terminal-previous-session",
            ),
        ],
    },
];
