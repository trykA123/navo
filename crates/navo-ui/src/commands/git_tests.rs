use super::git::{is_git, name_of};
use super::{Keymap, by_name, entries};
use crate::git::GitAction;

#[test]
fn the_five_actions_have_a_command_and_the_menu_is_the_only_one_with_a_key() {
    let keymap = Keymap::builtin();
    for name in ["git.commit", "git.pull", "git.push", "git.log", "git.diff"] {
        assert!(is_git(name), "{name}");
        assert!(keymap.binds().iter().all(|bind| bind.spec.name != name));
    }
    let menu: Vec<_> = keymap
        .binds()
        .iter()
        .filter(|bind| bind.spec.name == "git.menu")
        .collect();
    assert_eq!(menu.len(), 1);
    assert_eq!(menu[0].keys, ["alt-g"]);
}

#[test]
fn alt_g_is_scoped_to_the_file_list_for_git_and_the_map_for_grouping() {
    let keymap = Keymap::builtin();
    let context_of = |name: &str| {
        let found = keymap.binds().iter().find(|bind| bind.spec.name == name);
        found.map(|bind| (bind.context.clone(), bind.keys.clone()))
    };
    let (git, git_keys) = context_of("git.menu").expect("git.menu");
    let (map, map_keys) = context_of("space.next_grouping").expect("grouping");
    assert_eq!(git_keys, map_keys);
    assert_eq!(git, "FileList");
    assert_ne!(git, map);
}

#[test]
fn every_git_action_maps_to_a_registered_command() {
    for action in [
        GitAction::Commit,
        GitAction::Pull,
        GitAction::Push,
        GitAction::Log,
        GitAction::Diff,
    ] {
        let name = name_of(action).expect("name");
        assert!(by_name(name).is_some(), "{name}");
    }
    assert_eq!(name_of(GitAction::Open), None);
    assert!(entries().iter().filter(|e| is_git(e.spec.name)).count() == 6);
}
