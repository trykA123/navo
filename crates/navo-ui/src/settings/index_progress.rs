//! What Settings shows about the index service: per-volume state, entries, files and folders.

use gpui_kit::SharedString;
use navo_index::pipe::{MemoryReport, Part, Status};

use crate::browser::TotalsState;

const UPDATING: u8 = 100;

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum IndexState {
    #[default]
    NotRunning,
    Building,
    Updating,
    Ready,
}

impl IndexState {
    #[must_use]
    pub fn text(self) -> SharedString {
        let id = match self {
            Self::NotRunning => "settings-index-state-not-running",
            Self::Building => "settings-index-state-building",
            Self::Updating => "settings-index-state-updating",
            Self::Ready => "settings-index-state-ready",
        };
        navo_l10n::tr(id).into()
    }
}

/// Files and folders under a volume root; the root itself is not counted.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Split {
    pub files: u64,
    pub folders: u64,
}

impl Split {
    fn plus(self, other: Self) -> Self {
        Self {
            files: self.files.saturating_add(other.files),
            folders: self.folders.saturating_add(other.folders),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct VolumeProgress {
    pub root: SharedString,
    pub state: IndexState,
    /// Every name the index holds on this volume: files, folders and the root.
    pub entries: u64,
    pub percent: Option<u8>,
    /// The file/folder split, once the service answers folder totals for the root.
    pub split: Option<Split>,
}

impl VolumeProgress {
    #[must_use]
    pub fn new(root: &str, entries: u64, progress: Option<u8>) -> Self {
        let (state, percent) = match progress {
            None => (IndexState::Ready, Some(100)),
            Some(UPDATING..) => (IndexState::Updating, None),
            Some(0) => (IndexState::Building, None),
            Some(percent) => (IndexState::Building, Some(percent)),
        };
        Self {
            root: root.trim_end_matches(['\\', '/']).to_owned().into(),
            state,
            entries,
            percent,
            split: None,
        }
    }
}

/// What Settings shows about the index service: one row per volume plus the service memory.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct IndexProgress {
    pub volumes: Vec<VolumeProgress>,
    pub memory: Option<u64>,
    pub is_reachable: bool,
}

impl IndexProgress {
    /// Builds the view from the service replies; `None` status means the service did not answer.
    #[must_use]
    pub fn of(status: Option<&Status>, memory: Option<&MemoryReport>) -> Self {
        let Some(status) = status else {
            return Self::default();
        };
        Self {
            volumes: status
                .volumes
                .iter()
                .map(|volume| VolumeProgress::new(&volume.root, volume.entries, volume.progress))
                .collect(),
            memory: memory.and_then(service_bytes),
            is_reachable: true,
        }
    }

    #[must_use]
    pub fn state(&self) -> IndexState {
        if !self.is_reachable {
            return IndexState::NotRunning;
        }
        [IndexState::Building, IndexState::Updating]
            .into_iter()
            .find(|wanted| self.volumes.iter().any(|volume| volume.state == *wanted))
            .unwrap_or(IndexState::Ready)
    }

    /// The first volume that is not ready, the one the service works on now.
    #[must_use]
    pub fn current(&self) -> Option<&VolumeProgress> {
        let state = self.state();
        self.volumes
            .iter()
            .find(|volume| volume.state == state && state != IndexState::Ready)
    }

    /// Every name in the index, files, folders and roots together.
    #[must_use]
    pub fn entries(&self) -> u64 {
        self.volumes
            .iter()
            .fold(0, |total, volume| total.saturating_add(volume.entries))
    }

    /// The indexed volume roots, in service order, as the totals query paths.
    #[must_use]
    pub fn roots(&self) -> Vec<String> {
        self.volumes
            .iter()
            .map(|volume| format!("{}\\", volume.root))
            .collect()
    }

    /// Takes the root totals, one per volume in order. Only complete totals give a split; busy,
    /// missing or still-building (incomplete) answers leave the volume on its item count.
    #[must_use]
    pub fn with_totals(mut self, totals: &[TotalsState]) -> Self {
        if totals.len() != self.volumes.len() {
            return self;
        }
        for (volume, total) in self.volumes.iter_mut().zip(totals) {
            volume.split = match total {
                TotalsState::Ready(found) if found.complete => Some(Split {
                    files: u64::from(found.files),
                    folders: u64::from(found.folders),
                }),
                TotalsState::Ready(_) | TotalsState::Busy | TotalsState::Unavailable => None,
            };
        }
        self
    }

    /// The file/folder split over all volumes, known only when every volume has one.
    #[must_use]
    pub fn split(&self) -> Option<Split> {
        self.volumes
            .iter()
            .try_fold(Split::default(), |sum, volume| {
                Some(sum.plus(volume.split?))
            })
    }
}

fn service_bytes(report: &MemoryReport) -> Option<u64> {
    report
        .parts
        .iter()
        .find(|part| part.part == Part::Process.code())
        .map(|part| part.bytes)
}

#[cfg(test)]
mod tests {
    use navo_index::pipe::{MemoryPart, VolumeStatus};

    use super::*;

    fn status(volumes: &[(&str, u64, Option<u8>)]) -> Status {
        Status {
            volumes: volumes
                .iter()
                .map(|(root, entries, progress)| VolumeStatus {
                    root: (*root).to_owned(),
                    entries: *entries,
                    progress: *progress,
                    errors: Vec::new(),
                    total_files: None,
                })
                .collect(),
        }
    }

    fn memory(bytes: u64) -> MemoryReport {
        MemoryReport {
            parts: vec![
                MemoryPart {
                    part: Part::Names.code(),
                    bytes: 1,
                    items: 0,
                },
                MemoryPart {
                    part: Part::Process.code(),
                    bytes,
                    items: 0,
                },
            ],
            busy: false,
            process_id: None,
            volumes: None,
            builds: None,
        }
    }

    #[test]
    fn a_silent_service_is_not_running_with_no_rows() {
        let view = IndexProgress::of(None, None);
        assert_eq!(view.state(), IndexState::NotRunning);
        assert!(view.volumes.is_empty() && view.current().is_none());
        assert_eq!(view.entries(), 0);
    }

    #[test]
    fn progress_values_map_to_states_and_percent() {
        let view = IndexProgress::of(
            Some(&status(&[
                ("C:", 10, Some(0)),
                ("D:\\", 20, Some(100)),
                ("E:", 30, None),
                ("F:", 40, Some(42)),
            ])),
            None,
        );
        let found: Vec<_> = view
            .volumes
            .iter()
            .map(|volume| (volume.state, volume.percent))
            .collect();
        assert_eq!(
            found,
            [
                (IndexState::Building, None),
                (IndexState::Updating, None),
                (IndexState::Ready, Some(100)),
                (IndexState::Building, Some(42)),
            ]
        );
        assert_eq!(view.volumes[1].root.as_ref(), "D:");
    }

    #[test]
    fn a_building_volume_carries_its_percent() {
        assert_eq!(VolumeProgress::new("C:", 0, Some(34)).percent, Some(34));
        assert_eq!(VolumeProgress::new("C:", 0, Some(0)).percent, None);
    }

    #[test]
    fn building_wins_over_updating_and_names_the_current_volume() {
        let view = IndexProgress::of(
            Some(&status(&[
                ("C:", 5, Some(100)),
                ("D:", 6, None),
                ("E:", 7, Some(0)),
            ])),
            None,
        );
        assert_eq!(view.state(), IndexState::Building);
        assert_eq!(view.current().map(|v| v.root.as_ref()), Some("E:"));
        assert_eq!(view.entries(), 18);
    }

    #[test]
    fn all_volumes_ready_is_ready_with_no_current_volume() {
        let view = IndexProgress::of(Some(&status(&[("C:", 1, None), ("D:", 2, None)])), None);
        assert_eq!(view.state(), IndexState::Ready);
        assert!(view.current().is_none());
    }

    #[test]
    fn an_answer_with_no_volumes_is_ready_and_zero_files_are_fine() {
        let view = IndexProgress::of(Some(&status(&[])), None);
        assert_eq!(view.state(), IndexState::Ready);
        let empty = IndexProgress::of(Some(&status(&[("C:", 0, Some(0))])), None);
        assert_eq!(empty.volumes[0].entries, 0);
        assert_eq!(empty.volumes[0].percent, None);
    }

    #[test]
    fn file_totals_saturate() {
        let view = IndexProgress::of(
            Some(&status(&[("C:", u64::MAX, None), ("D:", 5, None)])),
            None,
        );
        assert_eq!(view.entries(), u64::MAX);
    }

    fn root(files: u32, folders: u32) -> TotalsState {
        TotalsState::Ready(navo_index::pipe::FolderTotals {
            files,
            folders,
            complete: true,
            ..Default::default()
        })
    }

    #[test]
    fn incomplete_root_totals_of_a_building_drive_leave_no_split() {
        let status = status(&[("C:", 1234, Some(0)), ("D:", 9, None)]);
        let building = TotalsState::Ready(navo_index::pipe::FolderTotals::default());
        let view = IndexProgress::of(Some(&status), None).with_totals(&[building, root(5, 3)]);
        assert_eq!(view.volumes[0].split, None);
        assert_eq!(
            view.volumes[1].split,
            Some(Split {
                files: 5,
                folders: 3
            })
        );
        assert_eq!(view.split(), None);
    }

    #[test]
    fn root_totals_split_files_from_folders_per_volume_and_in_sum() {
        let view = IndexProgress::of(Some(&status(&[("C:", 31, None), ("D:\\", 9, None)])), None)
            .with_totals(&[root(20, 10), root(5, 3)]);
        assert_eq!(view.roots(), ["C:\\", "D:\\"]);
        let split = |files, folders| Some(Split { files, folders });
        assert_eq!(view.volumes[0].split, split(20, 10));
        assert_eq!(view.volumes[1].split, split(5, 3));
        assert_eq!(view.split(), split(25, 13));
    }

    #[test]
    fn a_busy_or_missing_root_total_leaves_no_split() {
        let status = status(&[("C:", 31, Some(0)), ("D:", 9, None)]);
        let view =
            IndexProgress::of(Some(&status), None).with_totals(&[TotalsState::Busy, root(5, 3)]);
        assert_eq!(view.volumes[0].split, None);
        assert!(view.volumes[1].split.is_some());
        assert_eq!(view.split(), None);
        let short = IndexProgress::of(Some(&status), None).with_totals(&[root(1, 1)]);
        assert_eq!(short.split(), None);
        assert_eq!(short.volumes[0].split, None);
    }

    #[test]
    fn service_memory_is_the_process_part_and_missing_for_an_old_service() {
        let status = status(&[("C:", 1, None)]);
        let view = IndexProgress::of(Some(&status), Some(&memory(4096)));
        assert_eq!(view.memory, Some(4096));
        let without = MemoryReport::default();
        assert_eq!(
            IndexProgress::of(Some(&status), Some(&without)).memory,
            None
        );
        assert_eq!(IndexProgress::of(Some(&status), None).memory, None);
    }
}
