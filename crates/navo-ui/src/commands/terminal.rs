//! Terminal module command registrations.

use super::Group;
use super::contexts::GLOBAL;

pub(super) const GROUPS: [Group; 1] = [Group {
    context: GLOBAL,
    commands: &[(
        "navo_window::ToggleTerminal",
        "terminal.toggle",
        "cmd-terminal-toggle",
    )],
}];
