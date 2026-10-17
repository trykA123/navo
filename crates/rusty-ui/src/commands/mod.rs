mod contexts;

#[cfg(test)]
mod tests;

use crate::app_menu::MenuCommand;

pub use contexts::{GLOBAL, is_within};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CommandSpec {
    pub name: &'static str,
    pub label: &'static str,
    pub context: &'static str,
    pub action: Option<&'static str>,
}

pub fn all() -> impl Iterator<Item = CommandSpec> {
    let commands: Vec<CommandSpec> = todo!();
    commands.into_iter()
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
    todo!("{command:?}")
}
