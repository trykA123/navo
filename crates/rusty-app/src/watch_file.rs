use std::path::Path;

use rusty_core::{Location, ProviderPath};
use rusty_fs::{LocalFs, Provider, Watch, WatchChange, WatchEvent};

pub fn watch_file(path: &Path, mut on_change: impl FnMut() + Send + 'static) -> Option<Watch> {
    let folder = Location::new(path.parent()?.to_str()?).ok()?;
    let name = path.file_name()?.to_str()?.to_owned();
    let sink = Box::new(move |event: WatchEvent| {
        if touches(&event, &name) {
            on_change();
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
