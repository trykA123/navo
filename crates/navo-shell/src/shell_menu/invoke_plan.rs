use std::path::{Path, PathBuf};

use super::{MenuError, MenuPaths};

/// How `InvokeCommand` names the command: the menu offset, or a verb string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Command<'a> {
    Offset(u32),
    Verb(&'a str),
}

/// The fields of `CMINVOKECOMMANDINFOEX` that depend on what the menu was made for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Plan<'a> {
    pub command: Command<'a>,
    /// Becomes `lpDirectory`: static verbs such as "Open Git Bash here" run in it.
    pub directory: Option<PathBuf>,
}

/// The offset wins: it is exact for this menu, while verb strings of static verbs are unreliable.
/// A verb is used only when the item has no offset in the menu.
pub(super) fn plan<'a>(
    paths: &MenuPaths,
    offset: Option<u32>,
    verb: Option<&'a str>,
) -> Result<Plan<'a>, MenuError> {
    let command = match (offset, verb) {
        (Some(offset), _) => Command::Offset(offset),
        (None, Some(verb)) => Command::Verb(verb),
        (None, None) => return Err(MenuError::NotShellCommand),
    };
    Ok(Plan {
        command,
        directory: directory_of(paths),
    })
}

fn directory_of(paths: &MenuPaths) -> Option<PathBuf> {
    match paths {
        MenuPaths::Background(folder) => Some(folder.clone()),
        MenuPaths::Items(items) => items
            .first()
            .and_then(|item| item.parent())
            .filter(|parent| *parent != Path::new(""))
            .map(Path::to_path_buf),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(paths: &[&str]) -> MenuPaths {
        MenuPaths::Items(paths.iter().map(PathBuf::from).collect())
    }

    #[test]
    fn a_background_menu_runs_in_the_folder_itself() {
        let paths = MenuPaths::Background(PathBuf::from("/data/project"));
        let plan = plan(&paths, Some(4), None).unwrap();
        assert_eq!(plan.directory, Some(PathBuf::from("/data/project")));
    }

    #[test]
    fn an_item_menu_runs_in_the_parent_of_the_selection() {
        let paths = items(&["/data/project/a.txt", "/data/project/b.txt"]);
        let plan = plan(&paths, Some(4), None).unwrap();
        assert_eq!(plan.directory, Some(PathBuf::from("/data/project")));
    }

    #[test]
    fn an_item_without_a_parent_has_no_directory() {
        for name in ["a.txt", "/"] {
            let planned = plan(&items(&[name]), Some(1), None).unwrap();
            assert_eq!(planned.directory, None, "{name}");
        }
    }

    #[test]
    fn the_offset_is_used_even_when_a_verb_is_known() {
        let paths = MenuPaths::Background(PathBuf::from("/data"));
        let plan = plan(&paths, Some(7), Some("git_shell")).unwrap();
        assert_eq!(plan.command, Command::Offset(7));
    }

    #[test]
    fn a_verb_is_used_only_without_an_offset() {
        let paths = items(&["/data/a.txt"]);
        let plan = plan(&paths, None, Some("properties")).unwrap();
        assert_eq!(plan.command, Command::Verb("properties"));
        assert_eq!(plan.directory, Some(PathBuf::from("/data")));
    }

    #[test]
    fn a_command_without_offset_or_verb_is_not_a_shell_command() {
        let paths = items(&["/data/a.txt"]);
        assert_eq!(plan(&paths, None, None), Err(MenuError::NotShellCommand));
    }
}
