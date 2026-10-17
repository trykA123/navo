use std::path::Path;

use rusty_fs::Watch;
use rusty_ui::window_view::KeymapNews;

pub const FILE: &str = "keymap.toml";

#[must_use]
pub fn load(_path: &Path) -> KeymapNews {
    todo!("P20 K4")
}

pub fn report(_news: &KeymapNews) {
    todo!("P20 K4")
}

pub fn watch(
    _path: &Path,
    _start: KeymapNews,
    _on_news: impl FnMut(KeymapNews) + Send + 'static,
) -> Option<Watch> {
    todo!("P20 K4")
}

#[cfg(test)]
mod tests;
