use std::fs::Metadata;
use std::path::Path;
use std::time::SystemTime;

use chrono::{DateTime, Local};
use rusty_core::rename::{FieldInfo, FieldProvider, Values};

static FIELDS: [FieldInfo; 5] = [
    FieldInfo::builtin("size", "rename-field-file-size"),
    FieldInfo::builtin("size_kb", "rename-field-file-size-kb"),
    FieldInfo::builtin("created", "rename-field-file-created"),
    FieldInfo::builtin("accessed", "rename-field-file-accessed"),
    FieldInfo::builtin("attrs", "rename-field-file-attrs"),
];

pub struct FileFields;

impl FieldProvider for FileFields {
    fn group(&self) -> &str {
        "file"
    }

    fn fields(&self) -> &[FieldInfo] {
        &FIELDS
    }

    fn read(&self, path: &Path) -> Values {
        let mut values = Values::default();
        let Ok(meta) = std::fs::metadata(path) else {
            return values;
        };
        values.insert("size", &meta.len().to_string());
        values.insert("size_kb", &meta.len().div_ceil(1024).to_string());
        if let Some(stamp) = meta.created().ok().map(stamp) {
            values.insert("created", &stamp);
        }
        if let Some(stamp) = meta.accessed().ok().map(stamp) {
            values.insert("accessed", &stamp);
        }
        values.insert("attrs", &attrs(path, &meta));
        values
    }
}

fn stamp(time: SystemTime) -> String {
    let local: DateTime<Local> = time.into();
    local.format("%Y-%m-%d %H.%M.%S").to_string()
}

#[cfg(windows)]
fn flags(_path: &Path, meta: &Metadata) -> [bool; 4] {
    use std::os::windows::fs::MetadataExt;
    let bits = meta.file_attributes();
    [0x1, 0x2, 0x4, 0x20].map(|bit| bits & bit != 0)
}

#[cfg(not(windows))]
fn flags(path: &Path, meta: &Metadata) -> [bool; 4] {
    let hidden = path
        .file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with('.'));
    [meta.permissions().readonly(), hidden, false, false]
}

fn attrs(path: &Path, meta: &Metadata) -> String {
    flags(path, meta)
        .into_iter()
        .zip(['R', 'H', 'S', 'A'])
        .filter_map(|(set, letter)| set.then_some(letter))
        .collect()
}
