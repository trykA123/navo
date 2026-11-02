use std::path::PathBuf;

use navo_ui::app_menu::Install;

use super::hooks;

#[test]
fn the_identity_names_the_version_and_the_exe_folder() {
    let exe = PathBuf::from("apps").join("Navo").join("navo.exe");
    let menu = hooks(Some(&exe));
    assert_eq!(menu.identity.version.as_ref(), env!("CARGO_PKG_VERSION"));
    assert_eq!(menu.identity.install, Install::Installed);
    let folder = PathBuf::from("apps").join("Navo");
    assert_eq!(menu.identity.folder.as_ref(), folder.to_string_lossy());
}
