//! Window keys for the setup wizard: Enter, Esc and Alt+letter, the footer buttons and the rail steps.

use super::{SetupAction, SetupPage, SetupView, Strings, access_keys};
use crate::SharedString;

/// A key the setup window handles itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupKey {
    Enter,
    Escape,
    Alt(char),
}

/// Where keyboard focus is when a key arrives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Window,
    Field,
    Control,
}

/// One footer button.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FooterButton {
    pub action: SetupAction,
    pub label: SharedString,
    pub primary: bool,
    pub enabled: bool,
}

/// The state of one rail step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepMark {
    Todo,
    Done,
    Current,
    Skipped,
    Failed,
}

/// Maps one key to the action it runs, or None when the window should leave it alone.
///
/// Enter runs the enabled primary button unless a button or check box has focus (it clicks
/// itself). Esc is Close on every page but Installing. Alt+letter runs the matching access key.
pub fn key_action(
    view: &SetupView,
    strings: &Strings,
    key: SetupKey,
    focus: Focus,
) -> Option<SetupAction> {
    match key {
        SetupKey::Enter if focus == Focus::Control => None,
        SetupKey::Enter => default_action(view),
        SetupKey::Escape => (view.page != SetupPage::Installing).then_some(SetupAction::Close),
        SetupKey::Alt(letter) => {
            let letter = letter.to_lowercase().next().unwrap_or(letter);
            access_keys(view, strings)
                .into_iter()
                .find_map(|(key, action)| (key == letter).then_some(action))
        }
    }
}

/// Returns the primary action when it is enabled: Next waits for its gate.
fn default_action(view: &SetupView) -> Option<SetupAction> {
    view.primary
        .filter(|action| *action != SetupAction::Next || view.can_next())
}

/// Lists the footer buttons left to right: Back, Close, then the primary button.
///
/// Refusal pages and Same draw their own buttons, so their footer is empty.
pub fn footer(view: &SetupView, strings: &Strings) -> Vec<FooterButton> {
    use SetupPage::{Failed, Finish, Folder, Installing, Licence, Shortcuts, Welcome};
    let button = |action, label: &SharedString, primary, enabled| FooterButton {
        action,
        label: label.clone(),
        primary,
        enabled,
    };
    let page = view.page;
    let mut row = Vec::new();
    if matches!(page, Licence | Folder | Shortcuts | Installing | Failed) {
        row.push(button(
            SetupAction::Back,
            &strings.back,
            false,
            view.can_back(),
        ));
    }
    if matches!(page, Welcome | Licence | Folder | Shortcuts | Installing) {
        row.push(button(
            SetupAction::Close,
            &strings.close,
            false,
            page != Installing,
        ));
    }
    if !matches!(
        page,
        Welcome | Licence | Folder | Shortcuts | Finish | Failed
    ) {
        return row;
    }
    let label = match view.primary {
        Some(SetupAction::Next) => &strings.next,
        Some(SetupAction::Primary) if page == Finish => &strings.finish,
        Some(SetupAction::Primary) => &strings.install,
        Some(SetupAction::Close) => &strings.close,
        _ => return row,
    };
    let enabled = default_action(view).is_some();
    row.extend(
        view.primary
            .map(|action| button(action, label, true, enabled)),
    );
    row
}

/// Marks the six rail steps (Welcome, Licence, Folder, Shortcuts, Installing, Finish).
///
/// An update skips Shortcuts; a refusal before the flow starts marks Welcome as failed.
pub fn steps(view: &SetupView) -> [StepMark; 6] {
    let (current, mark) = match view.page {
        SetupPage::Welcome | SetupPage::Same => (0, StepMark::Current),
        SetupPage::Licence => (1, StepMark::Current),
        SetupPage::Folder => (2, StepMark::Current),
        SetupPage::Shortcuts => (3, StepMark::Current),
        SetupPage::Installing => (4, StepMark::Current),
        SetupPage::Finish => (5, StepMark::Current),
        SetupPage::Failed | SetupPage::Blocked => (4, StepMark::Failed),
        SetupPage::Newer | SetupPage::Ambiguous | SetupPage::Elevated | SetupPage::NoPayload => {
            (0, StepMark::Failed)
        }
    };
    std::array::from_fn(|step| match step {
        3 if view.folder_locked => StepMark::Skipped,
        _ if step < current => StepMark::Done,
        _ if step == current => mark,
        _ => StepMark::Todo,
    })
}
