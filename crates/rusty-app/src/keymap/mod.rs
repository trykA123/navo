use std::fs;
use std::io;
use std::path::Path;

use rusty_core::Location;
use rusty_fs::Watch;
use rusty_store::StoreDirs;
use rusty_ui::commands::Keymap;
use rusty_ui::shortcuts::Saved;
use rusty_ui::window_view::{KeymapHooks, KeymapNews};

use crate::watch_file::watch_file;

mod save;

pub use save::saver;

pub const FILE: &str = "keymap.toml";

#[must_use]
pub fn load(path: &Path) -> KeymapNews {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => return KeymapNews::Invalid(format!("{FILE}: {error}")),
    };
    match checked(&text) {
        Ok(saved) => KeymapNews::Applied {
            keymap: saved.keymap,
            warnings: saved.warnings,
        },
        Err(message) => KeymapNews::Invalid(message),
    }
}

fn checked(text: &str) -> Result<Saved, String> {
    match Keymap::builtin().overlay(text) {
        Ok(overlaid) => Ok(Saved {
            keymap: overlaid.keymap,
            warnings: overlaid
                .warnings
                .iter()
                .map(|warning| format!("{FILE}, {warning}"))
                .collect(),
        }),
        Err(error) if error.line.is_some() => Err(format!("{FILE}, {error}")),
        Err(error) => Err(format!("{FILE}: {error}")),
    }
}

pub fn hooks(dirs: Option<&StoreDirs>) -> (KeymapHooks, Option<Watch>) {
    let Some(dirs) = dirs.filter(|dirs| dirs.ensure().is_ok()) else {
        return (KeymapHooks::none(), None);
    };
    let path = dirs.keymap_file();
    let path = path.as_path();
    let start = load(path);
    report(&start);
    let (hooks, sender) = KeymapHooks::channel(Some(start.clone()));
    let hooks = hooks.with_file(location_of(path), saver(path));
    let watching = watch(path, start, move |news| {
        let _ = sender.send(news);
    });
    if watching.is_none() {
        tracing::warn!(
            "the settings folder is not watched; keymap.toml changes apply at the next start"
        );
    }
    (hooks, watching)
}

pub fn report(news: &KeymapNews) {
    match news {
        KeymapNews::Applied { warnings, .. } => {
            for warning in warnings {
                tracing::warn!(problem = %warning, "user keymap applied with a warning");
            }
        }
        KeymapNews::Invalid(message) => {
            tracing::error!(problem = %message, "user keymap not applied");
        }
    }
}

pub fn watch(
    path: &Path,
    start: KeymapNews,
    mut on_news: impl FnMut(KeymapNews) + Send + 'static,
) -> Option<Watch> {
    let file = path.to_owned();
    let mut last = start;
    watch_file(path, move || {
        let news = load(&file);
        if news != last {
            report(&news);
            last = news.clone();
            on_news(news);
        }
    })
}

fn location_of(path: &Path) -> Option<Location> {
    Location::new(path.to_str()?).ok()
}

#[cfg(test)]
mod tests;
