//! The folder tree the compare tab draws: row tones, folder mixes, the filter and open folders.
//! No GPUI handle lives here; the view calls `lines` and `map_hunks` on each frame.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use sidewise_ui::browser::{CompareFilter, CompareKind, CompareRow, RowStatus};
use sidewise_ui::compare_diff::map::{HunkKind, MapHunk};

use super::folders_cells::{GutTag, Newer, SideCell, child_counts, gut_of, newer_of, side_cell};

/// The five Sidewise row tones, in mix-bar order (NC121).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwTone {
    Changed,
    OnlyLeft,
    OnlyRight,
    Conflict,
    Same,
}

/// File counts below a folder, indexed by `SwTone as usize`.
pub type Mix = [u32; 5];

/// One line of the tree as the view draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeLine {
    pub id: u64,
    pub path: String,
    pub name: String,
    pub depth: u16,
    pub folder: bool,
    pub tone: Option<SwTone>,
    pub mix: Mix,
    pub expanded: bool,
    pub left: Option<SideCell>,
    pub right: Option<SideCell>,
    pub newer: Option<Newer>,
    pub gut: Option<GutTag>,
}

/// Every compare row, the open folders and the filter.
pub struct FolderTree {
    rows: Vec<CompareRow>,
    expanded: BTreeSet<String>,
    filter: CompareFilter,
}

/// The tone of a file status; `None` while checking, not checked or unavailable.
#[must_use]
pub fn tone_of(status: RowStatus) -> Option<SwTone> {
    match status {
        RowStatus::Different => Some(SwTone::Changed),
        RowStatus::OnlyLeft => Some(SwTone::OnlyLeft),
        RowStatus::OnlyRight => Some(SwTone::OnlyRight),
        RowStatus::TypeConflict | RowStatus::CaseConflict => Some(SwTone::Conflict),
        RowStatus::Same | RowStatus::SameAfterRules | RowStatus::SameSizeTime => Some(SwTone::Same),
        RowStatus::Checking | RowStatus::NotChecked(_) | RowStatus::Unavailable(_) => None,
    }
}

/// The change map of `lines`: one hunk per changed, added or deleted file line.
#[must_use]
pub fn map_hunks(lines: &[TreeLine]) -> Arc<[MapHunk]> {
    let hunks: Vec<MapHunk> = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            let kind = match line.tone? {
                SwTone::Changed | SwTone::Conflict => HunkKind::Changed,
                SwTone::OnlyRight => HunkKind::Added,
                SwTone::OnlyLeft => HunkKind::Deleted,
                SwTone::Same => return None,
            };
            Some(MapHunk {
                rows: index as u32..index as u32 + 1,
                kind,
            })
        })
        .collect();
    hunks.into()
}

impl FolderTree {
    /// A tree over `rows` with nothing open and the `All` filter.
    #[must_use]
    pub fn new(rows: Vec<CompareRow>) -> Self {
        Self {
            rows,
            expanded: BTreeSet::new(),
            filter: CompareFilter::All,
        }
    }

    /// Replaces the rows; the filter stays, and so do the open folders that still exist.
    pub fn set_rows(&mut self, rows: Vec<CompareRow>) {
        self.rows = rows;
        let open = std::mem::take(&mut self.expanded);
        self.expanded = open.into_iter().filter(|p| self.has_folder(p)).collect();
    }

    /// Opens or closes the folder at `path`; false when no folder row has that path.
    pub fn toggle(&mut self, path: &str) -> bool {
        let open = !self.expanded.contains(path);
        self.set_expanded(path, open)
    }

    /// Opens or closes the folder at `path`; true when the open set changed.
    pub fn set_expanded(&mut self, path: &str, open: bool) -> bool {
        if !self.has_folder(path) {
            return false;
        }
        match open {
            true => self.expanded.insert(path.to_string()),
            false => self.expanded.remove(path),
        }
    }

    /// Sets which rows the panes list.
    pub fn set_filter(&mut self, filter: CompareFilter) {
        self.filter = filter;
    }

    /// The current filter.
    #[must_use]
    pub fn filter(&self) -> CompareFilter {
        self.filter
    }

    /// The visible lines in input order: kept rows whose ancestors are all open.
    #[must_use]
    pub fn lines(&self) -> Vec<TreeLine> {
        let mixes = folder_mixes(&self.rows);
        let counts = child_counts(&self.rows);
        self.rows
            .iter()
            .filter(|row| self.kept(row, &mixes))
            .filter(|row| self.visible(row))
            .map(|row| self.line_of(row, &mixes, &counts))
            .collect()
    }

    /// Counts every counting file row in all five tones, whatever the filter and open folders.
    #[must_use]
    pub fn totals(&self) -> Mix {
        let mut totals = [0; 5];
        for row in &self.rows {
            if let Some(tone) = counting_tone(row) {
                totals[tone as usize] += 1;
            }
        }
        totals
    }

    /// Opens the folder at `path` and every folder row below it; nothing when `path` is no folder.
    pub fn expand_all_under(&mut self, path: &str) {
        if !self.has_folder(path) {
            return;
        }
        let prefix = format!("{path}/");
        let below: Vec<String> = self
            .rows
            .iter()
            .filter(|row| is_folder(row) && row.path.starts_with(&prefix))
            .map(|row| row.path.clone())
            .collect();
        self.expanded.insert(path.to_string());
        self.expanded.extend(below);
    }

    /// Whether a folder row with `path` is known.
    fn has_folder(&self, path: &str) -> bool {
        self.rows
            .iter()
            .any(|row| is_folder(row) && row.path == path)
    }

    /// Whether the filter keeps `row`; folders are judged by their mix.
    fn kept(&self, row: &CompareRow, mixes: &HashMap<&str, Mix>) -> bool {
        match is_folder(row) {
            true => folder_kept(self.filter, mix_of(row, mixes)),
            false => self.filter.keeps(row),
        }
    }

    /// Whether every ancestor folder of `row` is open.
    fn visible(&self, row: &CompareRow) -> bool {
        ancestors(&row.path).all(|path| self.expanded.contains(path))
    }

    /// Builds the drawn line for `row`.
    fn line_of(
        &self,
        row: &CompareRow,
        mixes: &HashMap<&str, Mix>,
        counts: &HashMap<&str, [u32; 2]>,
    ) -> TreeLine {
        let folder = is_folder(row);
        let items = |index: usize| {
            folder.then(|| counts.get(row.path.as_str()).map_or(0, |pair| pair[index]))
        };
        TreeLine {
            id: row.id,
            path: row.path.clone(),
            name: row.name().to_string(),
            depth: depth_of(&row.path),
            folder,
            tone: if folder { None } else { tone_of(row.status) },
            mix: mix_of(row, mixes),
            expanded: folder && self.expanded.contains(&row.path),
            left: row.left.as_ref().map(|s| side_cell(s, items(0))),
            right: row.right.as_ref().map(|s| side_cell(s, items(1))),
            newer: newer_of(row),
            gut: if folder { None } else { gut_of(row.status) },
        }
    }
}

/// Whether `row` is a folder row: no file side and at least one folder side.
fn is_folder(row: &CompareRow) -> bool {
    !row.has_file()
        && (row
            .left
            .is_some_and(|side| side.kind == CompareKind::Folder)
            || row
                .right
                .is_some_and(|side| side.kind == CompareKind::Folder))
}

/// The tone of a row that counts toward a mix, or `None` when it does not count.
fn counting_tone(row: &CompareRow) -> Option<SwTone> {
    match row.has_file() {
        true => tone_of(row.status),
        false => None,
    }
}

/// The mix of every folder path in `rows`, built in one pass over each row's ancestors.
fn folder_mixes(rows: &[CompareRow]) -> HashMap<&str, Mix> {
    let mut mixes: HashMap<&str, Mix> = HashMap::new();
    for row in rows {
        let Some(tone) = counting_tone(row) else {
            continue;
        };
        for ancestor in ancestors(&row.path) {
            mixes.entry(ancestor).or_default()[tone as usize] += 1;
        }
    }
    mixes
}

/// The ancestor folder paths of `path`: `a` and `a/b` for `a/b/c`.
fn ancestors(path: &str) -> impl Iterator<Item = &str> {
    path.match_indices('/').map(|(index, _)| &path[..index])
}

/// The count of `/` in `path`, so a root child has depth 0.
fn depth_of(path: &str) -> u16 {
    path.matches('/').count() as u16
}

/// The mix a line shows: a folder's own counts, or zeros for a file.
fn mix_of(row: &CompareRow, mixes: &HashMap<&str, Mix>) -> Mix {
    match is_folder(row) {
        true => mixes.get(row.path.as_str()).copied().unwrap_or([0; 5]),
        false => [0; 5],
    }
}

/// Whether the filter's tones have a count in `mix`.
fn folder_kept(filter: CompareFilter, mix: Mix) -> bool {
    match filter {
        CompareFilter::All => true,
        CompareFilter::Differences => mix[..4].iter().any(|count| *count > 0),
        CompareFilter::Orphans => mix[1..3].iter().any(|count| *count > 0),
        CompareFilter::Same => mix[4] > 0,
    }
}
