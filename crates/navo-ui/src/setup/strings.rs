//! Installer texts as plain data; `&` marks an access key; loaded from the l10n catalog (setup.ftl).

use crate::SharedString;

/// Every text the setup pages show. Templates use `{version}`, `{installed}`, `{path}`,
/// `{needed}`, `{free}`, `{time}` and `{log}` placeholders that the pages fill in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Strings {
    pub back: SharedString,
    pub next: SharedString,
    pub welcome_install: SharedString,
    pub welcome_blurb: SharedString,
    pub welcome_installed: SharedString,
    pub licence_title: SharedString,
    pub licence_text: SharedString,
    pub licence_accept: SharedString,
    pub licence_apache: SharedString,
    pub apache_url: SharedString,
    pub folder_title: SharedString,
    pub folder_label: SharedString,
    pub folder_browse: SharedString,
    pub folder_space: SharedString,
    pub folder_space_unknown: SharedString,
    pub folder_warning: SharedString,
    pub shortcuts_title: SharedString,
    pub shortcuts_start_menu: SharedString,
    pub shortcuts_start_menu_hint: SharedString,
    pub shortcuts_desktop: SharedString,
    pub shortcuts_desktop_hint: SharedString,
    pub installing_title: SharedString,
    pub installing_bar: SharedString,
    pub installing_elapsed: SharedString,
    pub installing_still: SharedString,
    pub installing_tips_title: SharedString,
    /// Four (key, text) pairs that cross-fade while Setup works.
    pub installing_tips: Vec<(SharedString, SharedString)>,
    pub finish_title: SharedString,
    /// Three (key, text) pairs: the first keys to try.
    pub finish_keys: Vec<(SharedString, SharedString)>,
    pub finish_start: SharedString,
    pub finish_open_folder: SharedString,
    pub finish_remove: SharedString,
    pub failed_title: SharedString,
    pub failed_log: SharedString,
    pub failed_copy: SharedString,
    pub failed_retry: SharedString,
    pub failed_leftover: SharedString,
    pub blocked_title: SharedString,
    pub blocked_text: SharedString,
    pub blocked_note: SharedString,
    pub blocked_retry: SharedString,
    pub same_title: SharedString,
    pub same_text: SharedString,
    pub same_open: SharedString,
    pub newer_title: SharedString,
    pub newer_text: SharedString,
    pub ambiguous_title: SharedString,
    pub ambiguous_text: SharedString,
    pub elevated_title: SharedString,
    pub elevated_text: SharedString,
    pub no_payload_title: SharedString,
    pub no_payload_text: SharedString,
    pub close: SharedString,
    /// Footer: Install (Folder in an update, Shortcuts) and Finish.
    pub install: SharedString,
    pub finish: SharedString,
    /// Rail: the window title, "Version {version}" and the first step's name.
    pub title: SharedString,
    pub rail_version: SharedString,
    pub step_welcome: SharedString,
}

impl Strings {
    /// Reads every setup text from the l10n catalog. Placeholders such as `{version}` stay
    /// in the text as literal `{name}` markers for the pages to fill.
    #[expect(clippy::too_many_lines, reason = "one field per text")]
    pub fn load() -> Self {
        let text = |id: &str| -> SharedString {
            let mut args = navo_l10n::FluentArgs::new();
            for name in PLACEHOLDERS {
                args.set(name, format!("{{{name}}}"));
            }
            navo_l10n::tr_args(id, &args).into()
        };
        Self {
            back: text("setup-back"),
            next: text("setup-next"),
            welcome_install: text("setup-welcome-install"),
            welcome_blurb: text("setup-welcome-blurb"),
            welcome_installed: text("setup-welcome-installed"),
            licence_title: text("setup-licence-title"),
            licence_text: include_str!("../../../navo-setup/assets/LICENSE.txt").into(),
            licence_accept: text("setup-licence-accept"),
            licence_apache: text("setup-licence-apache"),
            apache_url: text("setup-apache-url"),
            folder_title: text("setup-folder-title"),
            folder_label: text("setup-folder-label"),
            folder_browse: text("setup-folder-browse"),
            folder_space: text("setup-folder-space"),
            folder_space_unknown: text("setup-folder-space-unknown"),
            folder_warning: text("setup-folder-warning"),
            shortcuts_title: text("setup-shortcuts-title"),
            shortcuts_start_menu: text("setup-shortcuts-start-menu"),
            shortcuts_start_menu_hint: text("setup-shortcuts-start-menu-hint"),
            shortcuts_desktop: text("setup-shortcuts-desktop"),
            shortcuts_desktop_hint: text("setup-shortcuts-desktop-hint"),
            installing_title: text("setup-installing-title"),
            installing_bar: text("setup-installing-bar"),
            installing_elapsed: text("setup-installing-elapsed"),
            installing_still: text("setup-installing-still"),
            installing_tips_title: text("setup-installing-tips-title"),
            installing_tips: vec![
                (
                    text("setup-installing-tip-1-key"),
                    text("setup-installing-tip-1-text"),
                ),
                (
                    text("setup-installing-tip-2-key"),
                    text("setup-installing-tip-2-text"),
                ),
                (
                    text("setup-installing-tip-3-key"),
                    text("setup-installing-tip-3-text"),
                ),
                (
                    text("setup-installing-tip-4-key"),
                    text("setup-installing-tip-4-text"),
                ),
            ],
            finish_title: text("setup-finish-title"),
            finish_keys: vec![
                (
                    text("setup-finish-key-1-key"),
                    text("setup-finish-key-1-text"),
                ),
                (
                    text("setup-finish-key-2-key"),
                    text("setup-finish-key-2-text"),
                ),
                (
                    text("setup-finish-key-3-key"),
                    text("setup-finish-key-3-text"),
                ),
            ],
            finish_start: text("setup-finish-start"),
            finish_open_folder: text("setup-finish-open-folder"),
            finish_remove: text("setup-finish-remove"),
            failed_title: text("setup-failed-title"),
            failed_log: text("setup-failed-log"),
            failed_copy: text("setup-failed-copy"),
            failed_retry: text("setup-failed-retry"),
            failed_leftover: text("setup-failed-leftover"),
            blocked_title: text("setup-blocked-title"),
            blocked_text: text("setup-blocked-text"),
            blocked_note: text("setup-blocked-note"),
            blocked_retry: text("setup-blocked-retry"),
            same_title: text("setup-same-title"),
            same_text: text("setup-same-text"),
            same_open: text("setup-same-open"),
            newer_title: text("setup-newer-title"),
            newer_text: text("setup-newer-text"),
            ambiguous_title: text("setup-ambiguous-title"),
            ambiguous_text: text("setup-ambiguous-text"),
            elevated_title: text("setup-elevated-title"),
            elevated_text: text("setup-elevated-text"),
            no_payload_title: text("setup-no-payload-title"),
            no_payload_text: text("setup-no-payload-text"),
            close: text("setup-close"),
            install: text("setup-install"),
            finish: text("setup-finish"),
            title: text("setup-window-title"),
            rail_version: text("setup-rail-version"),
            step_welcome: text("setup-step-welcome"),
        }
    }

    /// Returns the English texts; the same as [`Strings::load`] while only en-US exists.
    pub fn english() -> Self {
        Self::load()
    }
}

/// Template placeholders the pages fill in after loading.
const PLACEHOLDERS: [&str; 7] = [
    "version",
    "installed",
    "path",
    "needed",
    "free",
    "time",
    "log",
];
