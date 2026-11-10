//! Installer texts as plain data; `&` marks an access key. W12a fills these from setup.ftl.

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
}

impl Strings {
    /// Returns the English placeholder texts used until setup.ftl exists.
    pub fn english() -> Self {
        Self {
            back: "&Back".into(),
            next: "&Next".into(),
            welcome_install: "Install Navo {version}".into(),
            welcome_blurb: "Setup puts Navo in a folder you choose. No administrator rights are needed.".into(),
            welcome_installed: "Navo {installed} is installed in {path}. Setup updates it there. Your settings and shortcuts stay.".into(),
            licence_title: "Licence".into(),
            licence_text: include_str!("../../../navo-setup/assets/LICENSE.txt").into(),
            licence_accept: "I &accept the licence".into(),
            licence_apache: "Read the Apache-2.0 &licence text".into(),
            apache_url: "https://www.apache.org/licenses/LICENSE-2.0.txt".into(),
            folder_title: "Install folder".into(),
            folder_label: "Install folder".into(),
            folder_browse: "B&rowse…".into(),
            folder_space: "Needs {needed}, {free} free".into(),
            folder_space_unknown: "Disk space is not known yet".into(),
            folder_warning: "Setup creates this folder. Uninstalling Navo deletes it with everything inside, so keep your own files elsewhere.".into(),
            shortcuts_title: "Shortcuts".into(),
            shortcuts_start_menu: "&Start menu".into(),
            shortcuts_start_menu_hint: "Find Navo by typing its name".into(),
            shortcuts_desktop: "&Desktop".into(),
            shortcuts_desktop_hint: "An icon on your desktop".into(),
            installing_title: "Installing Navo".into(),
            installing_bar: "Installing".into(),
            installing_elapsed: "Elapsed {time}".into(),
            installing_still: "Setup is still working… A slow disk or a virus scanner can take a while. Details are in {log}.".into(),
            installing_tips_title: "While you wait".into(),
            installing_tips: vec![
                ("Ctrl+K".into(), "One box for files, folders, commands and tabs.".into()),
                ("F5 / F6".into(), "Copy and move in the background. Ctrl+Z undoes them.".into()),
                ("Ctrl+L".into(), "Pick a pane layout, up to four panes.".into()),
                ("Ctrl+F7".into(), "Compare two folders, then Enter on a file for its diff.".into()),
            ],
            finish_title: "Navo is installed in {path}.".into(),
            finish_keys: vec![
                ("Ctrl+K".into(), "find anything".into()),
                ("F1".into(), "every shortcut".into()),
                ("Shift+F1".into(), "a short guide".into()),
            ],
            finish_start: "&Start Navo".into(),
            finish_open_folder: "&Open folder".into(),
            finish_remove: "Remove Navo from Windows Settings → Apps.".into(),
            failed_title: "Setup could not finish".into(),
            failed_log: "Log".into(),
            failed_copy: "Copy &details".into(),
            failed_retry: "&Retry".into(),
            failed_leftover: "Setup left files in {path}. Delete this folder before you run Setup again.".into(),
        }
    }
}
