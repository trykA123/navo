//! Plain installer display data; the setup shell maps its flow and runner into this contract.

use std::{path::PathBuf, rc::Rc, time::Duration};

use crate::primitives::{TextFieldState, access_key};
use crate::tokens;
use crate::{AnyElement, App, IntoElement, Window};
use gpui_kit::{Entity, RenderOnce};

mod frame;
mod installing;
pub mod keys;
#[cfg(test)]
mod keys_tests;
mod pages_a;
#[cfg(test)]
mod pages_a_tests;
mod pages_b;
#[cfg(test)]
mod pages_b_tests;
mod pages_c;
#[cfg(test)]
mod pages_c_tests;
mod strings;
#[cfg(test)]
mod tests;

pub use frame::render_frame;
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

impl SetupProgress {
    /// Timing only, for callers that have no verified percentage to show.
    pub fn from_elapsed(elapsed: Duration, still_working: bool) -> Self {
        Self {
            elapsed,
            still_working,
        }
    }
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
    /// Formatted completion or recovery advice; Ambiguous shows it instead of the repair text
    /// (UnsafeName's distinct advice).
    pub notice: Option<String>,
    /// A failed fresh install's root that still holds files; Failed names it and turns Retry off.
    pub leftover_root: Option<PathBuf>,
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

    /// Whether Failed's Retry button is on: the flow allows it and no partial root is left.
    pub fn retry_enabled(&self) -> bool {
        self.can_retry && self.leftover_root.is_none()
    }

    /// Returns the effective Back gate; Installing cannot release the runner's page.
    pub fn can_back(&self) -> bool {
        self.can_back && self.page != SetupPage::Installing
    }
}

/// Lists the Alt+letter keys of the current page: the enabled footer buttons, then page controls.
pub fn access_keys(view: &SetupView, strings: &Strings) -> Vec<(char, SetupAction)> {
    let footer = keys::footer(view, strings);
    let buttons = footer
        .iter()
        .filter(|button| button.enabled)
        .map(|button| (&*button.label, button.action));
    buttons
        .chain(page_keys(view, strings))
        .filter_map(|(text, action)| access_key(text).map(|key| (key, action)))
        .collect()
}

/// Lists the labels and actions of the controls a page adds to Back and Next, in tab order.
fn page_keys<'a>(view: &SetupView, strings: &'a Strings) -> Vec<(&'a str, SetupAction)> {
    let toggle = SetupAction::Toggle;
    let close = (&*strings.close, SetupAction::Close);
    match view.page {
        SetupPage::Licence => vec![(&*strings.licence_accept, toggle(SetupCheck::AcceptLicence))],
        SetupPage::Folder if !view.folder_locked => {
            vec![(&*strings.folder_browse, SetupAction::Browse)]
        }
        SetupPage::Shortcuts => vec![
            (
                &*strings.shortcuts_start_menu,
                toggle(SetupCheck::StartMenu),
            ),
            (&*strings.shortcuts_desktop, toggle(SetupCheck::Desktop)),
        ],
        SetupPage::Finish => vec![
            (&*strings.finish_start, toggle(SetupCheck::StartNavo)),
            (&*strings.finish_open_folder, SetupAction::OpenFolder),
        ],
        SetupPage::Failed => {
            let mut keys = vec![(&*strings.failed_copy, SetupAction::CopyDetails)];
            if view.retry_enabled() {
                keys.push((&*strings.failed_retry, SetupAction::TryAgain));
            }
            keys
        }
        SetupPage::Same => vec![(&*strings.same_open, SetupAction::Primary), close],
        SetupPage::Blocked if view.can_retry => {
            vec![(&*strings.blocked_retry, SetupAction::TryAgain), close]
        }
        SetupPage::Blocked
        | SetupPage::Newer
        | SetupPage::Ambiguous
        | SetupPage::Elevated
        | SetupPage::NoPayload => vec![close],
        _ => Vec::new(),
    }
}

/// Renders the current page.
pub fn render_page(view: &SetupView, strings: &Strings, on_action: OnAction) -> AnyElement {
    Page {
        view: view.clone(),
        strings: strings.clone(),
        on_action,
        field: None,
    }
    .into_any_element()
}

/// One page, built on the current theme when it renders.
#[derive(IntoElement)]
pub(super) struct Page {
    pub view: SetupView,
    pub strings: Strings,
    pub on_action: OnAction,
    /// The editable Folder field; without it the folder shows as read-only text.
    pub field: Option<Entity<TextFieldState>>,
}

impl RenderOnce for Page {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx);
        let ctx = pages_a::Ctx {
            t,
            view: &self.view,
            strings: &self.strings,
            on_action: &self.on_action,
            reduce_motion: cx.reduce_motion(),
            field: self.field.as_ref(),
        };
        match self.view.page {
            SetupPage::Welcome => pages_a::welcome(&ctx),
            SetupPage::Licence => pages_a::licence(&ctx),
            SetupPage::Folder => pages_a::folder(&ctx),
            SetupPage::Shortcuts => pages_b::shortcuts(&ctx),
            SetupPage::Installing => installing::installing(&ctx),
            SetupPage::Finish => pages_b::finish(&ctx),
            SetupPage::Failed => pages_b::failed(&ctx),
            SetupPage::Blocked => pages_c::blocked(&ctx),
            SetupPage::Same => pages_c::same(&ctx),
            SetupPage::Newer => pages_c::newer(&ctx),
            SetupPage::Ambiguous => pages_c::ambiguous(&ctx),
            SetupPage::Elevated => pages_c::elevated(&ctx),
            SetupPage::NoPayload => pages_c::no_payload(&ctx),
        }
    }
}
