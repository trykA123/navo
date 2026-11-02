use std::fmt::Debug;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

#[cfg(windows)]
#[path = "check_windows.rs"]
mod windows;

pub(super) type Hay = (String, Vec<u8>);

pub(super) fn hay(name: &str, bytes: impl Into<Vec<u8>>) -> Hay {
    (name.to_owned(), bytes.into())
}

pub(super) fn debug_hay(name: &str, value: &dyn Debug) -> Hay {
    hay(name, format!("{value:?}"))
}

fn utf16(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn holds(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|window| window == needle)
}

/// Names of the haystacks that hold `needle` as UTF-8 or UTF-16LE.
pub(super) fn leaks(needle: &str, hays: &[Hay]) -> Vec<String> {
    let wide = utf16(needle);
    hays.iter()
        .filter(|(_, bytes)| holds(bytes, needle.as_bytes()) || holds(bytes, &wide))
        .map(|(name, _)| name.clone())
        .collect()
}

pub(super) fn assert_clean(needle: &str, hays: &[Hay]) {
    let found = leaks(needle, hays);
    assert!(found.is_empty(), "{needle} leaked into {found:?}");
}

/// Every file directly inside `dir`, with its bytes.
pub(super) fn files_in(dir: &Path) -> Vec<Hay> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.map(|entry| entry.unwrap().path()) {
        if entry.is_file() {
            let name = entry.file_name().unwrap().to_string_lossy().into_owned();
            #[cfg(windows)]
            let bytes = windows::read_file(&entry);
            #[cfg(not(windows))]
            let bytes = fs::read(&entry);
            found.push((
                name,
                bytes.unwrap_or_else(|error| panic!("{entry:?}: {error}")),
            ));
        }
    }
    found
}

/// The report's entry names and decompressed entries.
pub(super) fn zip_entries(zip: &Path) -> Vec<Hay> {
    let mut archive = zip::ZipArchive::new(File::open(zip).unwrap()).unwrap();
    let mut found = Vec::new();
    let mut names = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        let name = entry.name().unwrap().into_owned();
        found.push((format!("report/{name}"), bytes));
        names.push(name);
    }
    found.push(hay("report entry names", names.join("\n")));
    found
}
