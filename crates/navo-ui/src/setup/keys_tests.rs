//! Tests for the window keys: default and cancel buttons, Alt+letter keys, the footer and the rail.

use super::keys::{Focus, SetupKey, StepMark, footer, key_action, steps};
use super::{SetupAction, SetupCheck, SetupPage, SetupView, Strings, access_keys};

/// A view as the setup shell builds it for `page`, with its real primary button.
fn on(page: SetupPage) -> SetupView {
    let primary = match page {
        SetupPage::Welcome | SetupPage::Licence | SetupPage::Folder => Some(SetupAction::Next),
        SetupPage::Shortcuts | SetupPage::Finish | SetupPage::Same => Some(SetupAction::Primary),
        SetupPage::Installing => None,
        _ => Some(SetupAction::Close),
    };
    SetupView {
        page,
        primary,
        can_next: true,
        can_back: matches!(
            page,
            SetupPage::Licence | SetupPage::Folder | SetupPage::Shortcuts | SetupPage::Failed
        ),
        ..SetupView::default()
    }
}

fn press(view: &SetupView, key: SetupKey, focus: Focus) -> Option<SetupAction> {
    key_action(view, &Strings::load(), key, focus)
}

#[test]
fn enter_runs_the_enabled_primary_button() {
    let enter = |view: &SetupView| press(view, SetupKey::Enter, Focus::Window);
    assert_eq!(enter(&on(SetupPage::Welcome)), Some(SetupAction::Next));
    assert_eq!(enter(&on(SetupPage::Shortcuts)), Some(SetupAction::Primary));
    assert_eq!(enter(&on(SetupPage::Failed)), Some(SetupAction::Close));
    assert_eq!(enter(&on(SetupPage::Same)), Some(SetupAction::Primary));
    assert_eq!(enter(&on(SetupPage::Installing)), None);
    let unaccepted = SetupView {
        can_next: false,
        ..on(SetupPage::Licence)
    };
    assert_eq!(enter(&unaccepted), None, "Next is off until consent");
    let refused = SetupView {
        problem: Some("refused".into()),
        ..on(SetupPage::Folder)
    };
    assert_eq!(enter(&refused), None, "a shown problem blocks Next");
}

#[test]
fn enter_in_the_field_is_the_default_but_a_focused_control_keeps_it() {
    let folder = on(SetupPage::Folder);
    assert_eq!(
        press(&folder, SetupKey::Enter, Focus::Field),
        Some(SetupAction::Next)
    );
    assert_eq!(press(&folder, SetupKey::Enter, Focus::Control), None);
}

#[test]
fn esc_closes_every_page_except_installing() {
    for page in super::tests::ALL_PAGES {
        let expected = (page != SetupPage::Installing).then_some(SetupAction::Close);
        for focus in [Focus::Window, Focus::Field, Focus::Control] {
            assert_eq!(
                press(&on(page), SetupKey::Escape, focus),
                expected,
                "{page:?}"
            );
        }
    }
}

#[test]
fn alt_letters_reach_footer_and_page_controls() {
    let shortcuts = on(SetupPage::Shortcuts);
    let alt = |view: &SetupView, key| press(view, SetupKey::Alt(key), Focus::Field);
    assert_eq!(alt(&shortcuts, 'i'), Some(SetupAction::Primary));
    assert_eq!(alt(&shortcuts, 'I'), Some(SetupAction::Primary));
    assert_eq!(alt(&shortcuts, 'b'), Some(SetupAction::Back));
    assert_eq!(alt(&shortcuts, 'c'), Some(SetupAction::Close));
    assert_eq!(
        alt(&shortcuts, 'd'),
        Some(SetupAction::Toggle(SetupCheck::Desktop))
    );
    assert_eq!(alt(&shortcuts, 'n'), None, "Shortcuts has no Next");
    let installing = on(SetupPage::Installing);
    assert_eq!(alt(&installing, 'c'), None);
    assert_eq!(alt(&installing, 'b'), None);
    assert_eq!(alt(&on(SetupPage::Finish), 'f'), Some(SetupAction::Primary));
    assert_eq!(alt(&on(SetupPage::Folder), 'r'), Some(SetupAction::Browse));
}

#[test]
fn no_page_has_a_letter_clash_with_its_footer() {
    let strings = Strings::load();
    for page in super::tests::ALL_PAGES {
        for can_retry in [false, true] {
            let keys = access_keys(
                &SetupView {
                    can_retry,
                    ..on(page)
                },
                &strings,
            );
            let mut letters: Vec<char> = keys.iter().map(|(key, _)| *key).collect();
            letters.sort_unstable();
            letters.dedup();
            assert_eq!(letters.len(), keys.len(), "{page:?}: {keys:?}");
        }
    }
}

/// Lists the footer as (action, primary, enabled) for one view.
fn buttons(view: &SetupView) -> Vec<(SetupAction, bool, bool)> {
    footer(view, &Strings::load())
        .into_iter()
        .map(|b| (b.action, b.primary, b.enabled))
        .collect()
}

#[test]
fn the_footer_holds_back_close_and_the_primary_per_page() {
    use SetupAction::{Back, Close, Next, Primary};
    assert_eq!(
        buttons(&on(SetupPage::Welcome)),
        [(Close, false, true), (Next, true, true)]
    );
    assert_eq!(
        buttons(&on(SetupPage::Shortcuts)),
        [
            (Back, false, true),
            (Close, false, true),
            (Primary, true, true)
        ]
    );
    assert_eq!(
        buttons(&on(SetupPage::Installing)),
        [(Back, false, false), (Close, false, false)]
    );
    assert_eq!(buttons(&on(SetupPage::Finish)), [(Primary, true, true)]);
    assert_eq!(
        buttons(&on(SetupPage::Failed)),
        [(Back, false, true), (Close, true, true)]
    );
    assert!(
        buttons(&on(SetupPage::Same)).is_empty(),
        "the page draws its own"
    );
    let labels: Vec<String> = footer(&on(SetupPage::Finish), &Strings::load())
        .into_iter()
        .map(|b| b.label.to_string())
        .collect();
    assert_eq!(labels, ["&Finish"]);
}

#[test]
fn the_rail_marks_done_current_skipped_and_failed_steps() {
    use StepMark::{Current, Done, Failed, Skipped, Todo};
    assert_eq!(
        steps(&on(SetupPage::Folder)),
        [Done, Done, Current, Todo, Todo, Todo]
    );
    let update = SetupView {
        folder_locked: true,
        ..on(SetupPage::Installing)
    };
    assert_eq!(steps(&update), [Done, Done, Done, Skipped, Current, Todo]);
    assert_eq!(
        steps(&on(SetupPage::Failed)),
        [Done, Done, Done, Done, Failed, Todo]
    );
    assert_eq!(
        steps(&on(SetupPage::Elevated)),
        [Failed, Todo, Todo, Todo, Todo, Todo]
    );
}

#[test]
fn alt_r_is_off_while_the_retry_button_is_off_for_leftover_files() {
    let leftover = SetupView {
        can_retry: true,
        leftover_root: Some(r"C:\Programs\Navo".into()),
        ..on(SetupPage::Failed)
    };
    assert!(!leftover.retry_enabled());
    let alt_r = press(&leftover, SetupKey::Alt('r'), Focus::Window);
    assert_eq!(
        alt_r, None,
        "the Retry button is disabled, so its key is too"
    );
    let clean = SetupView {
        leftover_root: None,
        ..leftover
    };
    assert_eq!(
        press(&clean, SetupKey::Alt('r'), Focus::Window),
        Some(SetupAction::TryAgain)
    );
}
