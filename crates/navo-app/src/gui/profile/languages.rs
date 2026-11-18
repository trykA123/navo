//! Starts the background rule-language load at profile start.
//! It turns D14's folder list into the registry that `syntax_rules` installs.

use std::ffi::OsStr;
use std::path::Path;

use navo_store::StoreDirs;
use navo_ui::syntax_rules::{RuleDir, install_rules, load_rules};

/// Starts a `languages` thread that loads the UDL and own rule folders.
pub(super) fn install(dirs: Option<&StoreDirs>) {
    let Some(dirs) = dirs else {
        return;
    };
    let config = dirs.config.clone();
    let thread = std::thread::Builder::new()
        .name("languages".to_owned())
        .spawn(move || {
            let folders = folders(&config, std::env::var_os("APPDATA").as_deref());
            let (registry, errors) = load_rules(&folders);
            for error in errors {
                tracing::warn!(
                    path = %error.path.display(),
                    error = %error.error,
                    "language file skipped"
                );
            }
            install_rules(registry);
        });
    if let Err(error) = thread {
        tracing::warn!(%error, "starting the language loader failed");
    }
}

/// Returns the Notepad++ UDL folder then the Navo languages folder.
/// An empty or relative `appdata` counts as absent, so no current-directory path is read.
fn folders(config: &Path, appdata: Option<&OsStr>) -> Vec<RuleDir> {
    let mut folders = Vec::new();
    if let Some(appdata) = appdata.map(Path::new).filter(|path| path.is_absolute()) {
        folders.push(RuleDir::Udl(
            appdata.join("Notepad++").join("userDefineLangs"),
        ));
    }
    folders.push(RuleDir::Own(config.join("languages")));
    folders
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use navo_store::StoreDirs;

    use super::*;

    #[test]
    fn folders_put_appdata_udl_before_own() {
        let config = PathBuf::from("profile/config");
        let appdata = std::env::temp_dir().join("AppData");
        let udl = appdata.join("Notepad++").join("userDefineLangs");
        assert_eq!(
            folders(&config, Some(appdata.as_os_str())),
            [RuleDir::Udl(udl), RuleDir::Own(config.join("languages"))]
        );
    }

    #[test]
    fn folders_without_appdata_keep_only_own() {
        let config = PathBuf::from("profile/config");
        assert_eq!(
            folders(&config, None),
            [RuleDir::Own(config.join("languages"))]
        );
    }

    #[test]
    fn an_empty_or_relative_appdata_counts_as_absent() {
        let config = PathBuf::from("profile/config");
        for appdata in ["", "relative/AppData"] {
            assert_eq!(
                folders(&config, Some(OsStr::new(appdata))),
                [RuleDir::Own(config.join("languages"))],
                "{appdata:?}"
            );
        }
    }

    #[test]
    fn a_profile_folder_puts_own_languages_under_config() {
        let dirs = StoreDirs::in_profile(Path::new("Prüfung-ü"));
        let expected = PathBuf::from("Prüfung-ü").join("config").join("languages");
        assert_eq!(folders(&dirs.config, None), [RuleDir::Own(expected)]);
    }
}
