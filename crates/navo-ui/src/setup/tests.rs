//! Navigation guards for the installer display contract, independent of the setup engine.

use super::{SetupPage, SetupView};

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
