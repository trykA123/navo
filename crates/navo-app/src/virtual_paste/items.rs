use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use navo_core::{ItemKind, Location, NameError, ProviderPath, ScannedItem};
use navo_shell::{BadName, VirtualEntry};
use navo_ui::browser::Refused;

/// What the job needs to open one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FileEntry {
    pub index: u32,
    pub size: Option<u64>,
    pub modified: Option<SystemTime>,
}

/// The plan items for a paste, built from the descriptor alone.
#[derive(Debug, Default)]
pub(super) struct Planned {
    pub items: Vec<ScannedItem>,
    pub refused: Vec<Refused>,
    pub files: HashMap<Location, FileEntry>,
    pub items_only: bool,
}

struct Node {
    location: Location,
    file: Option<FileEntry>,
    modified: Option<SystemTime>,
    children: Vec<usize>,
}

#[derive(Default)]
struct Tree {
    nodes: Vec<Node>,
    tops: Vec<usize>,
    /// Lower-case relative path to node, because Windows names ignore case.
    by_key: HashMap<String, usize>,
}

/// Turns descriptor entries into pre-order items below `root`. Folders the descriptor only
/// implies are added; a refused or repeated name skips that entry with its reason.
pub(super) fn plan_items(entries: &[VirtualEntry], root: &Location) -> Planned {
    let mut tree = Tree::default();
    let mut planned = Planned::default();
    for entry in entries {
        let placed = match &entry.path {
            Ok(path) => tree.insert(root, path, entry),
            Err(bad) => Err(bad_name(*bad)),
        };
        if let Err(reason) = placed {
            planned.refused.push(Refused {
                name: entry.name.clone(),
                reason,
            });
        }
    }
    let mut pending: Vec<(usize, u32)> = tree.tops.iter().rev().map(|&at| (at, 0)).collect();
    while let Some((at, depth)) = pending.pop() {
        let node = &tree.nodes[at];
        planned.push(node, depth);
        pending.extend(node.children.iter().rev().map(|&child| (child, depth + 1)));
    }
    planned
}

impl Planned {
    fn push(&mut self, node: &Node, depth: u32) {
        let (kind, size) = match node.file {
            Some(file) => {
                self.items_only |= file.size.is_none();
                self.files.insert(node.location.clone(), file);
                (ItemKind::File, file.size.unwrap_or(0))
            }
            None => (ItemKind::Folder, 0),
        };
        self.items.push(ScannedItem {
            location: node.location.clone(),
            depth,
            kind,
            size,
            modified_ms: node.modified.and_then(millis),
        });
    }
}

impl Tree {
    fn insert(&mut self, root: &Location, path: &Path, entry: &VirtualEntry) -> Result<(), String> {
        let names: Vec<&str> = path.iter().filter_map(|part| part.to_str()).collect();
        let Some((last, folders)) = names.split_last() else {
            return Err(bad_name(BadName::Empty));
        };
        let mut parent = None;
        for depth in 0..folders.len() {
            let folder = self.folder(root, (parent, &names[..=depth]), None)?;
            parent = Some(folder);
        }
        if entry.is_dir {
            return self
                .folder(root, (parent, &names), entry.modified)
                .map(drop);
        }
        let file = FileEntry {
            index: entry.index,
            size: entry.size,
            modified: entry.modified,
        };
        let key = key_of(&names);
        if self.by_key.contains_key(&key) {
            return Err(navo_l10n::tr("paste-virtual-same-name"));
        }
        let location = self.location(root, parent, last)?;
        self.add(key, parent, (location, Some(file), entry.modified));
        Ok(())
    }

    /// The folder at `names`, made when missing. A file already there refuses the entry.
    fn folder(
        &mut self,
        root: &Location,
        (parent, names): (Option<usize>, &[&str]),
        modified: Option<SystemTime>,
    ) -> Result<usize, String> {
        let key = key_of(names);
        if let Some(&at) = self.by_key.get(&key) {
            if self.nodes[at].file.is_some() {
                return Err(navo_l10n::tr("paste-virtual-same-name"));
            }
            self.nodes[at].modified = self.nodes[at].modified.or(modified);
            return Ok(at);
        }
        let name = names.last().copied().unwrap_or_default();
        let location = self.location(root, parent, name)?;
        Ok(self.add(key, parent, (location, None, modified)))
    }

    fn location(
        &self,
        root: &Location,
        parent: Option<usize>,
        name: &str,
    ) -> Result<Location, String> {
        let folder = parent.map_or(root, |at| &self.nodes[at].location);
        folder
            .join(&ProviderPath::from_text(name))
            .map_err(|_| bad_name(BadName::Segment(NameError::DotName)))
    }

    fn add(
        &mut self,
        key: String,
        parent: Option<usize>,
        (location, file, modified): (Location, Option<FileEntry>, Option<SystemTime>),
    ) -> usize {
        let at = self.nodes.len();
        self.nodes.push(Node {
            location,
            file,
            modified,
            children: Vec::new(),
        });
        match parent {
            Some(parent) => self.nodes[parent].children.push(at),
            None => self.tops.push(at),
        }
        self.by_key.insert(key, at);
        at
    }
}

fn key_of(names: &[&str]) -> String {
    names.join("\\").to_lowercase()
}

fn millis(time: SystemTime) -> Option<i64> {
    match time.duration_since(UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_millis()).ok(),
        Err(before) => i64::try_from(before.duration().as_millis())
            .ok()
            .map(|ms| -ms),
    }
}

/// The reason shown for a refused name.
pub(super) fn bad_name(bad: BadName) -> String {
    let key = match bad {
        BadName::NotUnicode => "paste-name-not-unicode",
        BadName::Empty => "paste-name-empty",
        BadName::Absolute => "paste-name-absolute",
        BadName::Drive => "paste-name-drive",
        BadName::Stream => "paste-name-stream",
        BadName::Traversal => "paste-name-traversal",
        BadName::EmptySegment => "paste-name-empty-part",
        BadName::Segment(error) => return bad_segment(error),
    };
    navo_l10n::tr(key)
}

fn bad_segment(error: NameError) -> String {
    let key = match error {
        NameError::Empty => "paste-name-empty",
        NameError::Character(char) => {
            return navo_l10n::t!("paste-name-character", char = char.to_string());
        }
        NameError::DotName => "paste-name-dot",
        NameError::Trailing => "paste-name-trailing",
        NameError::Reserved => "paste-name-reserved",
        NameError::TooLong => "paste-name-too-long",
    };
    navo_l10n::tr(key)
}
