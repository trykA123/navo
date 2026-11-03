mod cache;
mod git;
mod handle;
mod local;
mod rules;
mod settings;

use std::fs;
use std::path::Path;

use navo_core::{Location, ProviderPath};

fn location(path: &Path) -> Location {
    let text = path.to_str().unwrap_or_default();
    Location::local(ProviderPath::from_text(text)).unwrap()
}

fn write(root: &Path, rel: &str, bytes: &[u8]) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}
