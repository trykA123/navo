use std::path::Path;

use rusty_ui::app_menu::{Identity, Install};
use rusty_ui::window_view::AppMenuHooks;

pub fn hooks(exe: Option<&Path>) -> AppMenuHooks {
    AppMenuHooks {
        identity: identity(exe),
    }
}

fn identity(exe: Option<&Path>) -> Identity {
    let folder = exe
        .and_then(Path::parent)
        .map(|folder| folder.to_string_lossy().into_owned())
        .unwrap_or_default();
    Identity {
        version: env!("CARGO_PKG_VERSION").into(),
        install: Install::Installed,
        folder: folder.into(),
    }
}

#[cfg(test)]
mod tests;
