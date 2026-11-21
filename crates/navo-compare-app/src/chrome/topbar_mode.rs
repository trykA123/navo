//! Topbar mode visibility rules, action variants, and prototype pill definitions.

/// Which comparison screen owns the topbar; determines which controls are mounted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TopbarMode {
    Start,
    #[default]
    Folders,
    File,
    Git,
    Stream,
    Large,
}

/// Which topbar action fired; the shell registers one callback for all of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Search,
    Theme,
    Accounts,
    View,
    Keys,
    Sync,
}

impl TopbarMode {
    /// Returns whether `action` is mounted in this screen mode.
    pub const fn shows(self, action: Action) -> bool {
        match action {
            Action::Search => matches!(self, Self::Folders),
            Action::View => matches!(self, Self::Start | Self::Folders),
            Action::Sync => {
                matches!(
                    self,
                    Self::Folders | Self::File | Self::Stream | Self::Large
                )
            }
            Action::Theme | Action::Keys => true,
            // Alpha ships no accounts, so its pill stays unmounted.
            Action::Accounts => false,
        }
    }
}

/// One action pill: its element id, its label, its key hint and the action it fires.
pub(super) struct Pill {
    pub(super) id: &'static str,
    pub(super) label: &'static str,
    pub(super) hint: Option<&'static str>,
    pub(super) action: Action,
}

/// The action pills, in prototype order after the search filter.
#[rustfmt::skip]
pub(super) const PILLS: [Pill; 5] = [
    Pill { id: "top-theme", label: "Theme", hint: Some("Alt+T"), action: Action::Theme },
    Pill { id: "top-accounts", label: "Accounts", hint: None, action: Action::Accounts },
    Pill { id: "top-view", label: "View", hint: Some("Alt+V"), action: Action::View },
    Pill { id: "top-keys", label: "Keys", hint: Some("?"), action: Action::Keys },
    Pill { id: "top-sync", label: "Sync", hint: Some("Alt+S"), action: Action::Sync },
];

#[cfg(test)]
mod tests {
    use super::{Action, TopbarMode};

    /// The View pill is mounted on Start and Folders only.
    #[test]
    fn the_view_pill_shows_on_start_and_folders_only() {
        for mode in [
            TopbarMode::Start,
            TopbarMode::Folders,
            TopbarMode::File,
            TopbarMode::Git,
            TopbarMode::Stream,
            TopbarMode::Large,
        ] {
            let expected = matches!(mode, TopbarMode::Start | TopbarMode::Folders);
            assert_eq!(mode.shows(Action::View), expected, "View pill on {mode:?}");
        }
    }

    /// The Theme pill shows in every mode; Accounts stays hidden.
    #[test]
    fn the_theme_pill_shows_in_every_mode() {
        for mode in [
            TopbarMode::Start,
            TopbarMode::Folders,
            TopbarMode::File,
            TopbarMode::Git,
            TopbarMode::Stream,
            TopbarMode::Large,
        ] {
            assert!(mode.shows(Action::Theme), "Theme pill on {mode:?}");
            assert!(!mode.shows(Action::Accounts), "no Accounts on {mode:?}");
        }
    }
}
