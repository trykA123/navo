use navo_core::Location;

use super::{GitAction, GitHooks, NoGit};

fn here() -> Location {
    Location::new("/tmp/repo").unwrap()
}

#[test]
fn no_git_offers_nothing() {
    let git = NoGit;
    git.observe(&here(), 1, vec!["a".into()]);
    assert!(git.badges(&here()).is_none());
    assert!(git.menu(&here()).items.is_empty());
    assert!(git.tools().is_empty());
    assert!(!git.can_diff(&here()));
}

#[test]
fn no_git_refuses_actions_and_diff() {
    let git = NoGit;
    assert!(git.run(&here(), GitAction::Commit).is_err());
    assert!(git.open_diff(&here(), &here()).is_err());
}
