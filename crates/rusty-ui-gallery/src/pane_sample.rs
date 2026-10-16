use std::rc::Rc;

use rusty_core::{Entry, Listing, ProviderPath};
use rusty_fs::{Attributes, EntryKind, FsEntry};

const MODIFIED_MS: i64 = 1_791_920_400_000;
const MINUTE_MS: i64 = 60_000;

pub const FKEYS: [(&str, &str); 9] = [
    ("F2", "Rename"),
    ("F3", "View"),
    ("F4", "Edit"),
    ("F5", "Copy"),
    ("F6", "Move"),
    ("F7", "New folder"),
    ("F8", "Delete"),
    ("F9", "Drawer"),
    ("F10", "Menu"),
];

fn entry(name: &str, kind: EntryKind, size: u64, minutes_ago: i64) -> FsEntry {
    FsEntry {
        name: ProviderPath::from_text(name),
        kind,
        size,
        modified_ms: Some(MODIFIED_MS - minutes_ago * MINUTE_MS),
        attributes: Attributes {
            hidden: name.starts_with('.'),
            ..Attributes::default()
        },
    }
}

pub fn rows() -> Rc<[FsEntry]> {
    let file = EntryKind::File;
    vec![
        entry("src", EntryKind::Folder, 0, 120),
        entry("tests", EntryKind::Folder, 2_722_951_735, 300),
        entry(".gitignore", file, 96, 40_000),
        entry("Cargo.lock", file, 41_233, 95),
        entry("Cargo.toml", file, 812, 95),
        entry("docs", EntryKind::Link, 0, 9_000),
        entry("fuzzy.rs", file, 5_632, 1_440),
        entry("layout.rs", file, 14_336, 18),
        entry("logo.png", file, 48_211, 7_200),
        entry("pane.rs", file, 7_372, 240),
        entry("README.md", file, 2_048, 2_880),
        entry("release-0.0.1.zip", file, 1_313_828_071_552, 4_320),
        entry("workspace.rs", file, 9_830, 240),
    ]
    .into()
}

pub fn listing(rows: &[FsEntry]) -> Listing {
    Listing::new(
        rows.iter()
            .map(|row| Entry {
                name: row.name.to_text(),
                is_folder: row.kind == EntryKind::Folder,
            })
            .collect(),
    )
}
