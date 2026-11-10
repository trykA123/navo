//! Installer texts as plain data; `&` marks an access key. W12a fills these from setup.ftl.

use crate::SharedString;

/// Every text the setup pages show. Templates use `{version}`, `{installed}`, `{path}`,
/// `{needed}` and `{free}` placeholders that the pages fill in.
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
        }
    }
}
