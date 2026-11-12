//! Navigation guards for the installer display contract, independent of the setup engine.

use std::time::Duration;

use super::{
    SetupAction, SetupCheck, SetupPage, SetupProgress, SetupStage, SetupView, Strings, access_keys,
};

#[test]
fn from_elapsed_leaves_the_bar_indeterminate() {
    let progress = SetupProgress::from_elapsed(Duration::from_secs(3), false);
    assert_eq!(progress.percent, None);
    assert_eq!(progress.stage, None);
}

#[test]
fn with_estimate_carries_the_verified_share_and_phase() {
    let progress = SetupProgress::from_elapsed(Duration::from_secs(3), false)
        .with_estimate(Some(42), Some(SetupStage::Extracting));
    assert_eq!(progress.percent, Some(42));
    assert_eq!(progress.stage, Some(SetupStage::Extracting));
    assert_eq!(progress.elapsed, Duration::from_secs(3));
    let cleared = progress.with_estimate(None, Some(SetupStage::Finalizing));
    assert_eq!(
        cleared.percent, None,
        "a stalled phase must clear the share"
    );
    assert_eq!(cleared.stage, Some(SetupStage::Finalizing));
}

#[test]
fn from_elapsed_keeps_the_time_and_the_deadline_flag() {
    let calm = SetupProgress::from_elapsed(Duration::from_secs(42), false);
    assert_eq!(calm.elapsed, Duration::from_secs(42));
    assert!(!calm.still_working);
    let slow = SetupProgress::from_elapsed(Duration::from_secs(180), true);
    assert_eq!(slow.elapsed, Duration::from_secs(180));
    assert!(slow.still_working);
}

#[test]
fn can_next_is_false_when_problem_is_set() {
    let mut view = SetupView {
        page: SetupPage::Folder,
        can_next: true,
        ..SetupView::default()
    };
    assert!(view.can_next());
    view.problem = Some(String::new());
    assert!(!view.can_next());
}

#[test]
fn back_is_false_on_installing() {
    let mut view = SetupView {
        page: SetupPage::Licence,
        can_back: true,
        ..SetupView::default()
    };
    assert!(view.can_back());
    view.page = SetupPage::Installing;
    assert!(!view.can_back());
}

pub(super) const ALL_PAGES: [SetupPage; 13] = [
    SetupPage::Welcome,
    SetupPage::Licence,
    SetupPage::Folder,
    SetupPage::Shortcuts,
    SetupPage::Installing,
    SetupPage::Finish,
    SetupPage::Failed,
    SetupPage::Blocked,
    SetupPage::Same,
    SetupPage::Newer,
    SetupPage::Ambiguous,
    SetupPage::Elevated,
    SetupPage::NoPayload,
];

#[test]
fn no_page_has_two_actions_on_one_letter() {
    let strings = Strings::load();
    for page in ALL_PAGES {
        let view = SetupView {
            page,
            can_next: true,
            can_back: true,
            ..SetupView::default()
        };
        let keys = access_keys(&view, &strings);
        let mut letters: Vec<char> = keys.iter().map(|(key, _)| *key).collect();
        letters.sort_unstable();
        letters.dedup();
        assert_eq!(letters.len(), keys.len(), "{page:?}: {keys:?}");
    }
}

#[test]
fn folder_lists_browse_until_the_field_is_locked() {
    let strings = Strings::load();
    let folder = SetupView {
        page: SetupPage::Folder,
        can_next: true,
        can_back: true,
        ..SetupView::default()
    };
    assert!(access_keys(&folder, &strings).contains(&('r', SetupAction::Browse)));
    let locked = SetupView {
        folder_locked: true,
        ..folder
    };
    let keys = access_keys(&locked, &strings);
    assert!(
        !keys
            .iter()
            .any(|(_, action)| *action == SetupAction::Browse)
    );
}

#[test]
fn licence_lists_accept_and_next_follows_its_gate() {
    let strings = Strings::load();
    let view = SetupView {
        page: SetupPage::Licence,
        can_back: true,
        ..SetupView::default()
    };
    let keys = access_keys(&view, &strings);
    assert!(keys.contains(&('b', SetupAction::Back)));
    assert!(keys.contains(&('a', SetupAction::Toggle(SetupCheck::AcceptLicence))));
    assert!(!keys.iter().any(|(_, action)| *action == SetupAction::Next));
}
