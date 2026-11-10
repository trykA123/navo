//! Plain installer display data; the setup shell maps its flow and runner into this contract.

use std::{path::PathBuf, rc::Rc, time::Duration};

use crate::primitives::access_key;
use crate::{AnyElement, App, IntoElement, ParentElement, Window, div};

mod strings;
#[cfg(test)]
mod tests;

pub use strings::Strings;

/// Callback the pages call with the user's intent; the setup shell owns the flow.
pub type OnAction = Rc<dyn Fn(SetupAction, &mut Window, &mut App)>;

/// The thirteen pages mirrored from the setup flow without an engine dependency.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SetupPage {
    #[default]
    Welcome,
    Licence,
    Folder,
    Shortcuts,
    Installing,
    Finish,
    Failed,
    Blocked,
    Same,
    Newer,
    Ambiguous,
    Elevated,
    NoPayload,
}

/// The four independent check boxes exposed by the flow's choices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupCheck {
    AcceptLicence,
    Desktop,
    StartMenu,
    StartNavo,
}

/// Intent sent to the shell; Primary is resolved against the current flow page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupAction {
    Next,
    Back,
    Primary,
    Browse,
    Toggle(SetupCheck),
    Close,
    TryAgain,
    OpenFolder,
    CopyDetails,
}

/// Runner timing in seconds through Duration, including its 180-second notice flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetupProgress {
    pub elapsed: Duration,
    pub still_working: bool,
}

/// Display values and engine gates; messages are supplied by the shell's localization.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SetupView {
    pub page: SetupPage,
    /// Shell gate: licence consent or a checked fresh folder, depending on the page.
    pub can_next: bool,
    pub can_back: bool,
    /// Enabled primary intent, including Install, Finish and Open Navo via Primary.
    pub primary: Option<SetupAction>,
    pub folder_text: String,
    pub problem: Option<String>,
    pub progress: Option<SetupProgress>,
    pub version: String,
    pub installed_version: Option<String>,
    pub folder_locked: bool,
    pub licence_accepted: bool,
    pub desktop: bool,
    pub start_menu: bool,
    pub start_navo: bool,
    pub can_retry: bool,
    /// Formatted completion or recovery advice, including UnsafeName's distinct advice.
    pub notice: Option<String>,
    pub log_paths: Vec<PathBuf>,
    pub details: String,
    /// Space values are bytes when the shell has obtained them.
    pub space_needed: Option<u64>,
    pub space_free: Option<u64>,
    /// True while Alt is held; access keys then show underlined.
    pub show_keys: bool,
}

impl SetupView {
    /// Returns the effective Next gate; a displayed problem always blocks navigation.
    pub fn can_next(&self) -> bool {
        self.can_next && self.problem.is_none() && self.page != SetupPage::Installing
    }

    /// Returns the effective Back gate; Installing cannot release the runner's page.
    pub fn can_back(&self) -> bool {
        self.can_back && self.page != SetupPage::Installing
    }
}

/// Lists the Alt+letter keys of the current page: Back and Next when enabled, plus page controls.
pub fn access_keys(view: &SetupView, strings: &Strings) -> Vec<(char, SetupAction)> {
    let mut keys = Vec::new();
    let mut add = |text: &str, action: SetupAction| {
        if let Some(key) = access_key(text) {
            keys.push((key, action));
        }
    };
    if view.can_back() {
        add(&strings.back, SetupAction::Back);
    }
    if view.can_next() {
        add(&strings.next, SetupAction::Next);
    }
    match view.page {
        SetupPage::Licence => add(
            &strings.licence_accept,
            SetupAction::Toggle(SetupCheck::AcceptLicence),
        ),
        SetupPage::Folder if !view.folder_locked => {
            add(&strings.folder_browse, SetupAction::Browse)
        }
        _ => {}
    }
    keys
}

/// Renders only the page name until the ordered page atoms supply their implementations.
pub fn render_page(view: &SetupView, _strings: &Strings, _on_action: OnAction) -> AnyElement {
    let name = match view.page {
        SetupPage::Welcome => "Welcome",
        SetupPage::Licence => "Licence",
        SetupPage::Folder => "Folder",
        SetupPage::Shortcuts => "Shortcuts",
        SetupPage::Installing => "Installing",
        SetupPage::Finish => "Finish",
        SetupPage::Failed => "Failed",
        SetupPage::Blocked => "Blocked",
        SetupPage::Same => "Same",
        SetupPage::Newer => "Newer",
        SetupPage::Ambiguous => "Ambiguous",
        SetupPage::Elevated => "Elevated",
        SetupPage::NoPayload => "NoPayload",
    };
    div().child(name).into_any_element()
}
