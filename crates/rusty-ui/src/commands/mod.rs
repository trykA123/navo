mod bind;
mod contexts;
mod keymap;
mod overlays;
mod structured;
mod viewer;
mod window;

#[cfg(test)]
mod tests;

use gpui_kit::App;

use crate::app_menu::MenuCommand;

pub(crate) use bind::bind_namespace;
#[cfg(test)]
use bind::key_bindings;
pub use contexts::{GLOBAL, is_within};
pub use keymap::{Bind, Keymap, KeymapError, normal_key, shown};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CommandSpec {
    pub name: &'static str,
    pub label: &'static str,
    pub context: &'static str,
    pub action: Option<&'static str>,
}

struct Group {
    context: &'static str,
    commands: &'static [(&'static str, &'static str, &'static str)],
}

const MENU_ONLY: [(&str, &str); 5] = [
    ("app.command_palette", "Command palette"),
    ("window.new", "New window"),
    ("workspace.list", "Workspaces"),
    ("help.key_map", "Help and key map"),
    ("app.quit", "Quit"),
];

pub fn all() -> impl Iterator<Item = CommandSpec> {
    let bound = [
        window::GROUPS.as_slice(),
        overlays::GROUPS.as_slice(),
        viewer::GROUPS.as_slice(),
        structured::GROUPS.as_slice(),
    ];
    let actions = bound.into_iter().flatten().flat_map(|group| {
        group
            .commands
            .iter()
            .map(|&(action, name, label)| CommandSpec {
                name,
                label,
                context: group.context,
                action: Some(action),
            })
    });
    let menu = MENU_ONLY.iter().map(|&(name, label)| CommandSpec {
        name,
        label,
        context: GLOBAL,
        action: None,
    });
    actions.chain(menu)
}

#[must_use]
pub fn for_action(action: &str) -> Option<CommandSpec> {
    all().find(|spec| spec.action == Some(action))
}

#[must_use]
pub fn by_name(name: &str) -> Option<CommandSpec> {
    all().find(|spec| spec.name == name)
}

#[must_use]
pub fn for_menu(command: MenuCommand) -> &'static str {
    match command {
        MenuCommand::CommandPalette => "app.command_palette",
        MenuCommand::NewWindow => "window.new",
        MenuCommand::Workspaces => "workspace.list",
        MenuCommand::Help => "help.key_map",
        MenuCommand::Quit => "app.quit",
        MenuCommand::AllSettings => "settings.toggle",
    }
}

#[must_use]
pub fn active(_: &App) -> &Keymap {
    Keymap::builtin()
}
