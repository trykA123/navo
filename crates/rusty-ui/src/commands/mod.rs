mod bind;
mod contexts;
mod keymap;
mod marks;
mod multi_rename;
mod overlays;
mod palette_hidden;
mod paths;
mod structured;
mod viewer;
mod window;

#[cfg(test)]
mod tests;

use gpui_kit::{App, Global};

use crate::app_menu::MenuCommand;

#[cfg(test)]
use bind::key_bindings;
pub(crate) use bind::{bind_namespace, bind_user_keys};
pub use contexts::{GLOBAL, is_within};
pub use keymap::{
    Bind, Keymap, KeymapError, KeymapWarning, Overlaid, UserKeymapError, normal_key, shown,
};

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

const MENU_ONLY: [(&str, &str); 4] = [
    ("window.new", "cmd-window-new"),
    ("workspace.list", "cmd-workspace-list"),
    ("help.key_map", "cmd-help-key-map"),
    ("app.quit", "cmd-app-quit"),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ArgKind {
    Path,
    Text,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CommandEntry {
    pub spec: CommandSpec,
    pub enabled: bool,
    pub in_palette: bool,
    pub arg: Option<ArgKind>,
}

const ARGUMENT_COMMANDS: [(&str, &str, &str, ArgKind); 4] = [
    (
        "pane.navigate",
        "command-center-arg-go-to-folder",
        "FerritePane",
        ArgKind::Path,
    ),
    (
        "pane.open_tab",
        "command-center-arg-open-folder-in-tab",
        "FerritePane",
        ArgKind::Path,
    ),
    (
        "selection.add_pattern",
        "command-center-arg-mark-by-pattern",
        "FileList",
        ArgKind::Text,
    ),
    (
        "selection.remove_pattern",
        "command-center-arg-unmark-by-pattern",
        "FileList",
        ArgKind::Text,
    ),
];

#[must_use]
pub fn entries() -> Vec<CommandEntry> {
    let bound = all().map(|spec| CommandEntry {
        spec,
        enabled: true,
        in_palette: !palette_hidden::HIDDEN.contains(&spec.name),
        arg: None,
    });
    let asking = ARGUMENT_COMMANDS
        .iter()
        .map(|&(name, label, context, kind)| CommandEntry {
            spec: CommandSpec {
                name,
                label,
                context,
                action: None,
            },
            enabled: true,
            in_palette: true,
            arg: Some(kind),
        });
    bound.chain(asking).collect()
}

pub fn all() -> impl Iterator<Item = CommandSpec> {
    let bound = [
        window::GROUPS.as_slice(),
        marks::GROUPS.as_slice(),
        paths::GROUPS.as_slice(),
        overlays::GROUPS.as_slice(),
        viewer::GROUPS.as_slice(),
        structured::GROUPS.as_slice(),
        multi_rename::GROUPS.as_slice(),
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

struct Active(Keymap);

impl Global for Active {}

#[must_use]
pub fn active(cx: &App) -> &Keymap {
    cx.try_global::<Active>()
        .map_or_else(|| Keymap::builtin(), |active| &active.0)
}

pub fn set_active(cx: &mut App, keymap: Keymap) {
    cx.set_global(Active(keymap));
}
