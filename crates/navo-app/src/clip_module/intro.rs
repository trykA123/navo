//! The first-open line: due once per profile, remembered by a marker file in the data folder.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

const SHOWN: &str = "clipboard-intro-shown";

pub(super) struct Intro {
    marker: Option<PathBuf>,
    /// Set after the first ask, so later opens do not touch the disk.
    asked: AtomicBool,
}

impl Intro {
    pub(super) fn new(data: Option<&Path>) -> Self {
        Self {
            marker: data.map(|data| data.join(SHOWN)),
            asked: AtomicBool::new(false),
        }
    }

    /// True once: the marker is written before the line is shown.
    pub(super) fn take(&self) -> bool {
        let Some(marker) = &self.marker else {
            return false;
        };
        !self.asked.swap(true, Ordering::AcqRel) && crate::settings::take_marker(marker)
    }
}

#[cfg(test)]
mod tests {
    use super::Intro;

    #[test]
    fn a_later_run_does_not_show_it_again() {
        let dir = tempfile::tempdir().unwrap();
        assert!(Intro::new(Some(dir.path())).take());
        assert!(!Intro::new(Some(dir.path())).take());
    }

    #[test]
    fn without_a_profile_folder_it_is_never_due() {
        assert!(!Intro::new(None).take());
    }
}
