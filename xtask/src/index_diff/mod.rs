//! `index-diff` compares a saved index snapshot with a live walk of one drive.
//! It reports paths the index misses or invents, and entries whose kind changed.

#[cfg(test)]
mod tests;

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Component, Path, PathBuf},
    process::ExitCode,
};

use navo_index::{
    file,
    table::{Flags, NameTable},
};

/// Options parsed from the `index-diff` command line.
#[derive(Debug, PartialEq, Eq)]
struct Options {
    snapshot: PathBuf,
    root: PathBuf,
    allow_missing: usize,
}

/// Counts from one snapshot-versus-walk comparison.
#[derive(Debug, Default, PartialEq, Eq)]
struct Report {
    entries: usize,
    walked: usize,
    missing: usize,
    extra: usize,
    wrong_kind: usize,
}

impl Report {
    /// True when the three difference counts exceed the allowed budget.
    fn failed(&self, allow_missing: usize) -> bool {
        self.missing + self.extra + self.wrong_kind > allow_missing
    }

    /// Render the one-line report the command prints.
    fn line(&self) -> String {
        format!(
            "entries={} walked={} missing={} extra={} wrong_kind={}",
            self.entries, self.walked, self.missing, self.extra, self.wrong_kind
        )
    }
}

/// Whether one path names a folder or a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Folder,
    File,
}

/// One walked path, its kind, and its hard-link key when the volume has one.
#[derive(Debug)]
struct Walked {
    path: String,
    kind: Kind,
    key: Option<FileKey>,
}

/// Native hard-link identity: volume serial and 16-byte file id (dev and ino on Unix).
type FileKey = (u64, [u8; 16]);

/// Walk progress: collected entries and folders still to read.
struct WalkState {
    entries: Vec<Walked>,
    folders: Vec<(PathBuf, String, bool)>,
}

impl WalkState {
    /// Start a walk at the root entry with the given display label.
    fn new(root: &Path, label: String) -> Self {
        Self {
            entries: vec![Walked {
                path: label.clone(),
                kind: Kind::Folder,
                key: None,
            }],
            folders: vec![(root.to_path_buf(), label, true)],
        }
    }

    /// Read one folder, appending its children and the folders still to visit.
    fn read_folder(&mut self, folder: &Path, label: &str, at_root: bool) -> Result<(), String> {
        let entries =
            fs::read_dir(folder).map_err(|error| format!("{}: {error}", folder.display()))?;
        for entry in entries {
            let entry = entry.map_err(|error| format!("{}: {error}", folder.display()))?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if at_root && skipped_name(&name) {
                continue;
            }
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            let link = is_link(&metadata);
            let kind = kind(&path, &metadata);
            let path_label = format!("{label}\\{name}");
            // A link is compared by its own entry; opening its target could fail or cross volumes.
            let key = if kind == Kind::File && !link {
                file_key(&path)?
            } else {
                None
            };
            self.entries.push(Walked {
                path: path_label.clone(),
                kind,
                key,
            });
            if kind == Kind::Folder && !link {
                self.folders.push((path, path_label, false));
            }
        }
        Ok(())
    }
}

/// Run `cargo xtask index-diff` and return the exit status.
pub(crate) fn run(args: &[String]) -> ExitCode {
    let options = match parse(args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("index-diff: {error}");
            eprintln!(
                "usage: cargo xtask index-diff --snapshot <file> --root <X:\\> [--allow-missing <n>]"
            );
            return ExitCode::from(2);
        }
    };
    match diff(&options) {
        Ok(report) => {
            println!("{}", report.line());
            if report.failed(options.allow_missing) {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(error) => {
            eprintln!("index-diff: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Parse the flags after `index-diff` into [`Options`].
fn parse(args: &[String]) -> Result<Options, String> {
    let (mut snapshot, mut root, mut allow_missing) = (None, None, None);
    let (pairs, remainder) = args.as_chunks::<2>();
    for pair in pairs {
        match pair[0].as_str() {
            "--snapshot" if snapshot.is_none() => snapshot = Some(PathBuf::from(&pair[1])),
            "--root" if root.is_none() => root = Some(PathBuf::from(&pair[1])),
            "--allow-missing" if allow_missing.is_none() => {
                allow_missing = Some(number(&pair[1])?);
            }
            option => return Err(format!("unknown, repeated or invalid option: {option}")),
        }
    }
    if !remainder.is_empty() {
        return Err("each option requires a value".to_owned());
    }
    Ok(Options {
        snapshot: snapshot.ok_or_else(|| "--snapshot <file> is required".to_owned())?,
        root: root.ok_or_else(|| "--root <X:\\> is required".to_owned())?,
        allow_missing: allow_missing.unwrap_or(0),
    })
}

/// Parse one non-negative count for `--allow-missing`.
fn number(value: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map_err(|_| "--allow-missing must be an integer >= 0".to_owned())
}

/// Load the snapshot, walk the root, and count the differences between them.
fn diff(options: &Options) -> Result<Report, String> {
    let (_header, table, _ids) = file::load_with_ids(&options.snapshot).map_err(|error| {
        format!(
            "cannot load snapshot {}: {error}",
            options.snapshot.display()
        )
    })?;
    let index = indexed_paths(&table)?;
    let walked = walk(&options.root)?;
    Ok(compare(index, walked))
}

/// Build the snapshot's full paths after dropping NTFS metadata folders.
fn indexed_paths(table: &NameTable) -> Result<HashMap<String, Kind>, String> {
    let root = table
        .path_with(0, '\\')
        .ok_or_else(|| "snapshot has no root entry".to_owned())?;
    let mut paths = HashMap::with_capacity(table.slots() as usize);
    for (id, _name) in table.iter() {
        let path = table
            .path_with(id, '\\')
            .ok_or_else(|| format!("snapshot entry {id} has no complete path"))?;
        if skipped(&path, &root) {
            continue;
        }
        let kind = match table.flags(id) {
            Some(flags) if flags.contains(Flags::FOLDER) => Kind::Folder,
            _ => Kind::File,
        };
        paths.insert(path, kind);
    }
    Ok(paths)
}

/// True when the path sits under `$`-metadata or `System Volume Information`.
fn skipped(path: &str, root: &str) -> bool {
    let Some(rest) = path.strip_prefix(root) else {
        return false;
    };
    if rest.is_empty() {
        return false;
    }
    let Some(rest) = rest.strip_prefix('\\') else {
        return false;
    };
    rest.split('\\').next().is_some_and(skipped_name)
}

/// True for root entries the index and the walk both leave out.
fn skipped_name(name: &str) -> bool {
    name.starts_with('$') || name.eq_ignore_ascii_case("System Volume Information")
}

/// Walk the root without following reparse points and collect every path.
fn walk(root: &Path) -> Result<Vec<Walked>, String> {
    let metadata =
        fs::symlink_metadata(root).map_err(|error| format!("{}: {error}", root.display()))?;
    if !metadata.is_dir() {
        return Err(format!("{} is not a folder", root.display()));
    }
    let label = display_path(root);
    let mut state = WalkState::new(root, label);
    while let Some((folder, label, at_root)) = state.folders.pop() {
        state.read_folder(&folder, &label, at_root)?;
    }
    Ok(state.entries)
}

#[cfg(windows)]
/// Read folder-ness from the reparse entry's own attributes, never its target.
fn kind(_path: &Path, metadata: &fs::Metadata) -> Kind {
    use std::os::windows::fs::{FileTypeExt, MetadataExt};
    let folder = metadata.file_attributes() & 0x10 != 0 || metadata.file_type().is_symlink_dir();
    if folder { Kind::Folder } else { Kind::File }
}

#[cfg(not(windows))]
/// Read folder-ness; a symlink counts as a folder when its target is one.
fn kind(path: &Path, metadata: &fs::Metadata) -> Kind {
    let folder = metadata.is_dir()
        || (metadata.file_type().is_symlink()
            && fs::metadata(path).is_ok_and(|target| target.is_dir()));
    if folder { Kind::Folder } else { Kind::File }
}

/// Read a native file identity for hard-link grouping, if the volume has one.
fn file_key(path: &Path) -> Result<Option<FileKey>, String> {
    navo_fs::native_identity(path)
        .map(|identity| identity.map(|identity| (identity.volume, identity.file)))
        .map_err(|error| format!("{}: {error}", path.display()))
}

#[cfg(windows)]
/// True when Windows marks this entry as a reparse point.
fn is_link(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
/// True when this entry is a symbolic link.
fn is_link(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

/// Render a native path with `\` separators, keeping its component case.
fn display_path(path: &Path) -> String {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => {
                parts.push(prefix.as_os_str().to_string_lossy().into_owned());
            }
            Component::RootDir => {
                if parts.is_empty() {
                    parts.push(String::new());
                }
            }
            Component::CurDir => {}
            Component::ParentDir => parts.push("..".to_owned()),
            Component::Normal(name) => parts.push(name.to_string_lossy().into_owned()),
        }
    }
    let path = parts.join("\\");
    normalize_drive_letter(path)
}

/// Upper-case a leading `x:` drive letter so `x:\` and `X:\` compare equal.
fn normalize_drive_letter(path: String) -> String {
    if path.as_bytes().get(1) == Some(&b':')
        && let Some(first) = path.chars().next()
        && first.is_ascii_lowercase()
    {
        let mut chars = path.chars();
        chars.next();
        return std::iter::once(first.to_ascii_uppercase())
            .chain(chars)
            .collect();
    }
    path
}

/// Group files by native identity and keep keyless entries apart.
fn walk_groups(walked: &[Walked]) -> (HashMap<FileKey, Vec<usize>>, Vec<usize>) {
    let mut groups: HashMap<FileKey, Vec<usize>> = HashMap::new();
    let mut loose = Vec::new();
    for (position, entry) in walked.iter().enumerate() {
        match entry.key {
            Some(key) => groups.entry(key).or_default().push(position),
            None => loose.push(position),
        }
    }
    (groups, loose)
}

/// Match one walked path, record it, and return whether its kind differs.
fn matched_path<'a>(
    index: &HashMap<String, Kind>,
    entry: &'a Walked,
    matched: &mut HashSet<&'a str>,
) -> Option<bool> {
    let kind = index.get(&entry.path)?;
    matched.insert(entry.path.as_str());
    Some(*kind != entry.kind)
}

/// Count missing, extra and wrong-kind paths between the index and the walk.
fn compare(index: HashMap<String, Kind>, walked: Vec<Walked>) -> Report {
    let (groups, loose) = walk_groups(&walked);
    let mut matched = HashSet::new();
    let mut extra = 0;
    let mut wrong_kind = 0;
    for positions in groups.values() {
        let hit = positions
            .iter()
            .any(|&position| index.contains_key(&walked[position].path));
        for &position in positions {
            match matched_path(&index, &walked[position], &mut matched) {
                Some(mismatch) => wrong_kind += usize::from(mismatch),
                None if !hit => extra += 1,
                None => {}
            }
        }
    }
    for position in loose {
        match matched_path(&index, &walked[position], &mut matched) {
            Some(mismatch) => wrong_kind += usize::from(mismatch),
            None => extra += 1,
        }
    }
    let missing = index
        .keys()
        .filter(|path| !matched.contains(path.as_str()))
        .count();
    Report {
        entries: index.len(),
        walked: walked.len(),
        missing,
        extra,
        wrong_kind,
    }
}
