use gpui_kit::SharedString;
use navo_index::pipe::{MemoryReport, Part, Status};

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

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct VolumeProgress {
    pub root: SharedString,
    pub state: IndexState,
    pub files: u64,
    pub percent: Option<u8>,
}

impl VolumeProgress {
    #[must_use]
    pub fn new(root: &str, files: u64, progress: Option<u8>) -> Self {
        let (state, percent) = match progress {
            None => (IndexState::Ready, Some(100)),
            Some(UPDATING..) => (IndexState::Updating, None),
            Some(0) => (IndexState::Building, None),
            Some(_) => (IndexState::Building, None),
        };
        Self {
            root: root.trim_end_matches(['\\', '/']).to_owned().into(),
            state,
            files,
            percent,
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

    #[must_use]
    pub fn files(&self) -> u64 {
        self.volumes
            .iter()
            .fold(0, |total, volume| total.saturating_add(volume.files))
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
        }
    }

    #[test]
    fn a_silent_service_is_not_running_with_no_rows() {
        let view = IndexProgress::of(None, None);
        assert_eq!(view.state(), IndexState::NotRunning);
        assert!(view.volumes.is_empty() && view.current().is_none());
        assert_eq!(view.files(), 0);
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
                (IndexState::Building, None),
            ]
        );
        assert_eq!(view.volumes[1].root.as_ref(), "D:");
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
        assert_eq!(view.files(), 18);
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
        assert_eq!(empty.volumes[0].files, 0);
        assert_eq!(empty.volumes[0].percent, None);
    }

    #[test]
    fn file_totals_saturate() {
        let view = IndexProgress::of(
            Some(&status(&[("C:", u64::MAX, None), ("D:", 5, None)])),
            None,
        );
        assert_eq!(view.files(), u64::MAX);
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
