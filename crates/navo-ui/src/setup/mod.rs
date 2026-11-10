//! Plain installer display data; the setup shell maps its flow and runner into this contract.

use std::{path::PathBuf, time::Duration};

use crate::{AnyElement, IntoElement, ParentElement, div};

#[cfg(test)]
mod tests;

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

/// Renders only the page name until the ordered page atoms supply their implementations.
pub fn render_page(view: &SetupView) -> AnyElement {
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
