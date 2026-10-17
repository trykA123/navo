use std::fs;
use std::io;
use std::path::Path;

use rusty_core::{Location, ProviderPath};
use rusty_fs::{LocalFs, Provider, Watch, WatchChange, WatchEvent};
use rusty_store::StoreDirs;
use rusty_ui::commands::Keymap;
use rusty_ui::window_view::{KeymapHooks, KeymapNews};

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
    match Keymap::builtin().overlay(&text) {
        Ok(overlaid) => KeymapNews::Applied {
            keymap: overlaid.keymap,
            warnings: overlaid
                .warnings
                .iter()
                .map(|warning| format!("{FILE}, {warning}"))
                .collect(),
        },
        Err(error) if error.line.is_some() => KeymapNews::Invalid(format!("{FILE}, {error}")),
        Err(error) => KeymapNews::Invalid(format!("{FILE}: {error}")),
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
    let folder = Location::new(path.parent()?.to_str()?).ok()?;
    let name = path.file_name()?.to_str()?.to_owned();
    let path = path.to_owned();
    let mut last = start;
    let sink = Box::new(move |event: WatchEvent| {
        if !touches(&event, &name) {
            return;
        }
        let news = load(&path);
        if news != last {
            report(&news);
            last = news.clone();
            on_news(news);
        }
    });
    LocalFs::new().watch(&folder, sink).ok()
}

fn touches(event: &WatchEvent, name: &str) -> bool {
    let is_it = |path: &ProviderPath| {
        path.as_str()
            .is_some_and(|it| it.eq_ignore_ascii_case(name))
    };
    match event {
        WatchEvent::Changed(changes) => changes.iter().any(|change| match change {
            WatchChange::Created(path)
            | WatchChange::Removed(path)
            | WatchChange::Modified(path) => is_it(path),
            WatchChange::Renamed { from, to } => is_it(from) || is_it(to),
        }),
        WatchEvent::Overflow => true,
        WatchEvent::Failed(_) => false,
    }
}

#[cfg(test)]
mod tests;
