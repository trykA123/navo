use std::collections::HashSet;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use gix_config::file::Metadata;
use gix_config::{File, Source};

const MAX_DEPTH: usize = 10;
const MAX_FILES: usize = 64;
const MAX_BYTES: u64 = 1 << 20;

pub(super) struct Loaded {
    pub(super) files: Vec<File>,
    pub(super) unreadable: bool,
    pub(super) special: bool,
}

/// Parses each root and every `include.path` and `includeIf.*.path` target, whatever the condition.
pub(super) fn load(roots: &[(PathBuf, Source)], home: Option<&Path>) -> Loaded {
    let mut loader = Loader {
        home,
        seen: HashSet::new(),
        loaded: Loaded {
            files: Vec::new(),
            unreadable: false,
            special: false,
        },
    };
    for (path, source) in roots {
        loader.visit(path, *source, 0);
    }
    loader.loaded
}

struct Loader<'a> {
    home: Option<&'a Path>,
    seen: HashSet<PathBuf>,
    loaded: Loaded,
}

enum Contents {
    Missing,
    Bytes(Vec<u8>),
    Unreadable,
    Special,
}

impl Loader<'_> {
    fn visit(&mut self, path: &Path, source: Source, depth: usize) {
        if depth > MAX_DEPTH || self.seen.len() >= MAX_FILES {
            self.loaded.unreadable = true;
            return;
        }
        if !self.seen.insert(path.to_path_buf()) {
            return;
        }
        let Some(file) = self.parse(path, source) else {
            return;
        };
        let includes = include_paths(&file);
        self.loaded.files.push(file);
        for raw in includes {
            match resolve(&raw, path, self.home) {
                Some(target) => self.visit(&target, source, depth + 1),
                None => self.loaded.unreadable = true,
            }
        }
    }

    fn parse(&mut self, path: &Path, source: Source) -> Option<File> {
        let bytes = match read_capped(path) {
            Contents::Missing => return None,
            Contents::Bytes(bytes) => bytes,
            Contents::Unreadable => {
                self.loaded.unreadable = true;
                return None;
            }
            Contents::Special => {
                self.loaded.unreadable = true;
                self.loaded.special = true;
                return None;
            }
        };
        let metadata = Metadata::from(source).at(path.to_path_buf());
        match File::from_bytes_no_includes(&bytes, metadata, Default::default()) {
            Ok(file) => Some(file),
            Err(_) => {
                self.loaded.unreadable = true;
                None
            }
        }
    }
}

/// Reads a regular file only. On Linux no magic link (`/proc/self/cwd`, `/dev/stdin`) resolves,
/// since Git would read it in its own process; other systems lack that check (D100e).
fn read_capped(path: &Path) -> Contents {
    let file = match open(path) {
        Ok(file) => file,
        Err(error) if is_missing(&error) => return Contents::Missing,
        Err(_) => return Contents::Unreadable,
    };
    match file.metadata() {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => return Contents::Special,
        Err(_) => return Contents::Unreadable,
    }
    let mut bytes = Vec::new();
    match file.take(MAX_BYTES + 1).read_to_end(&mut bytes) {
        Ok(read) if (read as u64) <= MAX_BYTES => Contents::Bytes(bytes),
        _ => Contents::Unreadable,
    }
}

#[cfg(target_os = "linux")]
fn open(path: &Path) -> io::Result<fs::File> {
    use rustix::fs::{CWD, Mode, ResolveFlags, openat2};

    let fd = openat2(
        CWD,
        path,
        read_flags(),
        Mode::empty(),
        ResolveFlags::NO_MAGICLINKS,
    )?;
    Ok(fs::File::from(fd))
}

#[cfg(all(unix, not(target_os = "linux")))]
fn open(path: &Path) -> io::Result<fs::File> {
    let fd = rustix::fs::open(path, read_flags(), rustix::fs::Mode::empty())?;
    Ok(fs::File::from(fd))
}

#[cfg(unix)]
fn read_flags() -> rustix::fs::OFlags {
    use rustix::fs::OFlags;

    OFlags::RDONLY | OFlags::NONBLOCK | OFlags::NOCTTY | OFlags::CLOEXEC
}

#[cfg(windows)]
fn open(path: &Path) -> io::Result<fs::File> {
    fs::File::open(path)
}

fn is_missing(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
    )
}

fn include_paths(file: &File) -> Vec<Vec<u8>> {
    file.sections()
        .filter(|section| {
            let header = section.header();
            let name = header.name();
            let conditional = header.subsection_name().is_some();
            (name.eq_ignore_ascii_case(b"include") && !conditional)
                || (name.eq_ignore_ascii_case(b"includeif") && conditional)
        })
        .flat_map(|section| section.values("path"))
        .map(|value| value.to_vec())
        .filter(|value| !value.is_empty())
        .collect()
}

fn resolve(raw: &[u8], including: &Path, home: Option<&Path>) -> Option<PathBuf> {
    if let Some(rest) = raw.strip_prefix(b"~/").or_else(|| raw.strip_prefix(b"~\\")) {
        return Some(home?.join(path_from_bytes(rest)?));
    }
    if raw.starts_with(b"~") || raw.starts_with(b"%(") {
        return None;
    }
    let path = path_from_bytes(raw)?;
    if path.is_absolute() {
        Some(path)
    } else {
        Some(including.parent()?.join(path))
    }
}

#[cfg(unix)]
fn path_from_bytes(bytes: &[u8]) -> Option<PathBuf> {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    Some(PathBuf::from(OsString::from_vec(bytes.to_vec())))
}

#[cfg(windows)]
fn path_from_bytes(bytes: &[u8]) -> Option<PathBuf> {
    String::from_utf8(bytes.to_vec()).ok().map(PathBuf::from)
}
