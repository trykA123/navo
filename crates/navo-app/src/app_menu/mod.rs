use std::path::Path;

use navo_ui::app_menu::{Identity, Install};
use navo_ui::window_view::AppMenuHooks;

pub fn hooks(exe: Option<&Path>, version: &str) -> AppMenuHooks {
    AppMenuHooks {
        identity: identity(exe, version),
    }
}

fn identity(exe: Option<&Path>, version: &str) -> Identity {
    let folder = exe
        .and_then(Path::parent)
        .map(|folder| folder.to_string_lossy().into_owned())
        .unwrap_or_default();
    Identity {
        version: version.to_owned().into(),
        install: Install::Installed,
        folder: folder.into(),
    }
}

#[cfg(test)]
mod tests;
