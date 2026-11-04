use std::path::{Path, PathBuf};

use super::Platform;
use crate::guard::{Recycler, ScratchRoot, ShellRecycler};
use crate::harness::picker::{PickerError, pick_folder};
use crate::index::IndexReport;
use crate::machine::{self, MachineSummary};
use crate::privacy::SafeText;
use crate::source::ReadOnlySource;
use crate::{measure, safe_text, timings};

/// This PC: the exe's own folder, the shell picker, the Recycle Bin and the real probes.
#[derive(Debug, Default)]
pub struct RealPlatform {
    recycler: ShellRecycler,
}

impl Platform for RealPlatform {
    fn exe_dir(&self) -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        exe.parent().map(Path::to_path_buf)
    }

    fn pick_folder(&mut self) -> Result<Option<PathBuf>, PickerError> {
        pick_folder()
    }

    fn recycler(&self) -> &dyn Recycler {
        &self.recycler
    }

    fn measure_host<'a>(&self, root: &'a ScratchRoot, exe: &Path) -> Box<dyn measure::Host + 'a> {
        Box::new(measure::RealHost::new(root, exe.to_path_buf()))
    }

    fn timings_host<'a>(
        &self,
        root: &'a ScratchRoot,
        exe: &Path,
        owner: Option<ReadOnlySource>,
    ) -> Box<dyn timings::Host + 'a> {
        Box::new(timings::RealHost::new(root, exe.to_path_buf(), owner))
    }

    fn index(&mut self, _root: &ScratchRoot, _exe: &Path) -> Result<IndexReport, SafeText> {
        // `navo --bench index --json` prints to stdout, and the one reviewed process start
        // discards it; reading it needs a reviewed capture there first.
        Err(safe_text!("index check not wired to the service yet"))
    }

    fn machine(&self, root: &Path) -> MachineSummary {
        machine::summarize(root)
    }
}
