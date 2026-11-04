use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::host::{Exe, Host, MeasureError, Session, Spec};
use super::layout;
use crate::guard::ScratchRoot;
use crate::probe::{Arg, CloseOutcome, CpuTime, Launched, Memory, PathFlag, ThreadSwitches};

const EXE_NAME: &str = if cfg!(windows) { "navo.exe" } else { "navo" };
const PDFIUM: &str = "pdfium.dll";

/// The real machine: the tester exe beside navo-bench and a guarded run root.
pub struct RealHost<'a> {
    root: &'a ScratchRoot,
    exe: PathBuf,
    began: Instant,
}

impl<'a> RealHost<'a> {
    pub fn new(root: &'a ScratchRoot, exe: PathBuf) -> Self {
        Self {
            root,
            exe,
            began: Instant::now(),
        }
    }

    fn exe_path(&self, exe: Exe) -> PathBuf {
        match exe {
            Exe::Original => self.exe.clone(),
            Exe::Copy(copy) => self
                .root
                .path()
                .join(layout::exe_folder(copy))
                .join(EXE_NAME),
        }
    }
}

impl Host for RealHost<'_> {
    fn prepare(&mut self) -> Result<(), MeasureError> {
        layout::build(self.root)
    }

    fn copy_exe(&mut self, copy: u32) -> Result<(), MeasureError> {
        let folder = layout::exe_folder(copy);
        self.root.create_dir(&folder)?;
        self.root.copy_in(&self.exe, &folder.join(EXE_NAME))?;
        let pdfium = self.exe.with_file_name(PDFIUM);
        if pdfium.is_file() {
            self.root.copy_in(&pdfium, &folder.join(PDFIUM))?;
        }
        Ok(())
    }

    fn launch(&mut self, spec: Spec) -> Result<Box<dyn Session>, MeasureError> {
        let profile = layout::profile(spec.run);
        self.root.create_dir(&profile)?;
        let metrics = layout::metrics(spec.run);
        let (left, right) = (Path::new(layout::FIXTURE_A), Path::new(layout::FIXTURE_B));
        let mut args = vec![
            Arg::Path(PathFlag::Profile, &profile),
            Arg::Path(PathFlag::Left, left),
            Arg::Path(PathFlag::Right, right),
        ];
        if spec.metrics {
            args.push(Arg::Path(PathFlag::Metrics, &metrics));
        }
        let launched = Launched::start(&self.exe_path(spec.exe), self.root, &args)?;
        Ok(Box::new(launched))
    }

    fn read_metrics(&mut self, run: u32) -> Option<String> {
        let full = self.root.path().join(layout::metrics(run));
        self.root.check(&full).ok()?;
        fs::read_to_string(full).ok()
    }

    fn pause(&mut self, time: Duration) {
        std::thread::sleep(time);
    }

    fn elapsed(&self) -> Duration {
        self.began.elapsed()
    }

    fn exe_size(&self) -> Result<u64, MeasureError> {
        fs::metadata(&self.exe)
            .map(|meta| meta.len())
            .map_err(|error| MeasureError::Io(error.kind()))
    }
}

impl Session for Launched {
    fn wait_for_window(&mut self, timeout: Duration) -> Result<Duration, MeasureError> {
        Launched::wait_for_window(self, timeout).map_err(Into::into)
    }

    fn memory(&self) -> Result<Memory, MeasureError> {
        Launched::memory(self).map_err(Into::into)
    }

    fn cpu_time(&self) -> Result<CpuTime, MeasureError> {
        Launched::cpu_time(self).map_err(Into::into)
    }

    fn thread_switches(&self) -> Result<Vec<ThreadSwitches>, MeasureError> {
        Launched::thread_switches(self).map_err(Into::into)
    }

    fn close(self: Box<Self>, deadline: Duration) -> Result<CloseOutcome, MeasureError> {
        Launched::close(*self, deadline).map_err(Into::into)
    }
}
